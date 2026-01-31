//
//  Database.swift
//  psh
//
//  Created by Pat Nakajima on 1/27/26.
//

import Foundation
import GRDB

struct PushNotification: Codable, Identifiable, FetchableRecord, PersistableRecord {
    var id: Int64?
    var apnsID: String?
    var title: String?
    var subtitle: String?
    var body: String?
    var category: String?
    var badge: Int?
    var data: String?
    var receivedAt: Date

    static let databaseTableName = "notifications"

    init(
        id: Int64? = nil,
        apnsID: String? = nil,
        title: String? = nil,
        subtitle: String? = nil,
        body: String? = nil,
        category: String? = nil,
        badge: Int? = nil,
        data: String? = nil,
        receivedAt: Date
    ) {
        self.id = id
        self.apnsID = apnsID
        self.title = title
        self.subtitle = subtitle
        self.body = body
        self.category = category
        self.badge = badge
        self.data = data
        self.receivedAt = receivedAt
    }

    mutating func didInsert(_ inserted: InsertionSuccess) {
        id = inserted.rowID
    }
}

extension PushNotification {
    static func matching(_ searchText: String, in db: Database) throws -> [PushNotification] {
        if searchText.isEmpty {
            return try PushNotification.order(Column("receivedAt").desc).fetchAll(db)
        }

        let escapedSearch = searchText
            .replacingOccurrences(of: "\"", with: "\"\"")
        let ftsQuery = "\"\(escapedSearch)\"*"

        return try PushNotification
            .filter(sql: """
                id IN (SELECT rowid FROM notifications_fts WHERE notifications_fts MATCH ?)
                """, arguments: [ftsQuery])
            .order(Column("receivedAt").desc)
            .fetchAll(db)
    }
}

final class AppDatabase: Sendable {
    static let shared: AppDatabase = {
        do {
            let url = try FileManager.default
                .url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
                .appendingPathComponent("psh.sqlite")
            let dbPool = try DatabasePool(path: url.path)
            let db = AppDatabase(dbPool)
            try db.migrate()
            return db
        } catch {
            fatalError("Failed to initialize database: \(error)")
        }
    }()

    private let dbPool: DatabasePool

    init(_ dbPool: DatabasePool) {
        self.dbPool = dbPool
    }

    func migrate() throws {
        var migrator = DatabaseMigrator()

        migrator.registerMigration("createNotifications") { db in
            try db.create(table: "notifications") { t in
                t.autoIncrementedPrimaryKey("id")
                t.column("apnsID", .text)
                t.column("title", .text)
                t.column("subtitle", .text)
                t.column("body", .text)
                t.column("category", .text)
                t.column("badge", .integer)
                t.column("data", .text)
                t.column("receivedAt", .datetime).notNull()
            }

            try db.create(virtualTable: "notifications_fts", using: FTS5()) { t in
                t.synchronize(withTable: "notifications")
                t.column("title")
                t.column("subtitle")
                t.column("body")
            }
        }

        try migrator.migrate(dbPool)
    }

    func save(_ notification: inout PushNotification) throws {
        try dbPool.write { db in
            try notification.save(db)
        }
    }

    func allNotifications() throws -> [PushNotification] {
        try dbPool.read { db in
            try PushNotification.order(Column("receivedAt").desc).fetchAll(db)
        }
    }

    func search(_ query: String) throws -> [PushNotification] {
        try dbPool.read { db in
            try PushNotification.matching(query, in: db)
        }
    }

    func deleteAll() throws {
        try dbPool.write { db in
            _ = try PushNotification.deleteAll(db)
        }
    }
}
