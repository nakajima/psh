# psh

`psh` lets you send a push to all of your apple devices. I wanted to use ntfy but apns is weird and claude exists.

it has no auth whatsoever. use tailscale i guess.

it consists of:

- a SwiftUI app (`psh/`) that registers for APNs and displays received notifications
- a Rust server (`server/`) that stores devices and sends APNs notifications
- an optional Rust CLI (`psh-cli/`) to send pushes and inspect server stats

## Prerequisites

- Xcode (for the app)
- Rust toolchain + Cargo (for server/CLI)
- Apple Developer APNs credentials (`.p8` key, key ID, team ID, topic)
- Docker (optional, for containerized server)

## Quick Start

### 1) App setup (Xcode config)

```bash
./setup.sh
```

This creates `Config.xcconfig` from your Team ID and bundle identifier.

### 2) Run the server locally in `server/`

```bash
export APNS_KEY_PATH=/absolute/path/to/AuthKey.p8
export APNS_KEY_ID=YOUR_KEY_ID
export APNS_TEAM_ID=YOUR_TEAM_ID
export APNS_TOPIC=your.bundle.id

cargo run
```

Server listens on `http://localhost:3000`.

### 3) Use the CLI in `psh-cli/

```bash
cargo run --server http://localhost:3000 ping
cargo run --server http://localhost:3000 stats
cargo run --server http://localhost:3000 send --title "Hello" --body "From psh-cli"
```

### 4) Run the app

Open `psh.xcodeproj` in Xcode and run the `psh` target on a device/simulator.

Note: `APIClient` currently defaults to `https://psh` in `psh/APIClient.swift`.  
If you want to use a local server, update that `baseURL`.

## Docker Server (Optional)

`docker-compose.yml` runs the server on port `3000` and persists DB data in `psh/data`.

```bash
mkdir -p psh/data psh/apns
# place your APNs key at psh/apns/AuthKey.p8

APNS_KEY_ID=YOUR_KEY_ID \
APNS_TEAM_ID=YOUR_TEAM_ID \
APNS_TOPIC=your.bundle.id \
docker compose up --build server
```

## API Endpoints

- `POST /register`: register/update a device token
- `POST /send`: send a push to all registered devices
- `GET /stats`: device/push counters
- `GET /pushes?installation_id=...`: push history for an installation
- `GET /pushes/:id`: detailed push record

## Development Commands

```bash
# Rust tests
cargo test --manifest-path server/Cargo.toml
cargo test --manifest-path psh-cli/Cargo.toml

# iOS tests (fastlane)
bundle exec fastlane ios test
```
