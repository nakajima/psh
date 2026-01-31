# Push Notification Server

A Rust server for sending Apple Push Notifications (APNs) via HTTP/2.

## API Endpoints

### POST /register

Register a device token for push notifications.

**Request Body:**

```json
{
  "device_token": "string",
  "environment": "sandbox" | "production",
  "device_name": "string (optional)",
  "device_type": "string (optional)",
  "os_version": "string (optional)",
  "app_version": "string (optional)"
}
```

**Response:**

```json
{
  "success": true,
  "message": "Device registered successfully"
}
```

**Error Response:**

```json
{
  "success": false,
  "error": "Error description"
}
```

### POST /send

Send a push notification to a registered device.

**Request Body:**

```json
{
  "device_token": "string (required)",

  "title": "string (optional)",
  "subtitle": "string (optional)",
  "body": "string (optional)",
  "launch_image": "string (optional)",

  "title_loc_key": "string (optional)",
  "title_loc_args": ["string"] (optional),
  "loc_key": "string (optional)",
  "loc_args": ["string"] (optional),

  "badge": number (optional),
  "sound": "string" | { "name": "string", "critical": boolean, "volume": number } (optional),

  "content_available": boolean (optional),
  "mutable_content": boolean (optional),
  "category": "string (optional)",

  "priority": number (optional, 1-10),
  "collapse_id": "string (optional)",
  "expiration": number (optional, unix timestamp),

  "data": { "key": "value" } (optional)
}
```

**Response:**

```json
{
  "success": true,
  "message": "Notification sent successfully",
  "apns_id": "uuid-string"
}
```

**Error Response:**

```json
{
  "success": false,
  "error": "Error description"
}
```

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `APNS_KEY_PATH` | Yes | - | Path to the APNs authentication key (.p8 file) |
| `APNS_KEY_ID` | Yes | - | Key ID from Apple Developer Portal |
| `APNS_TEAM_ID` | Yes | - | Team ID from Apple Developer Portal |
| `APNS_TOPIC` | Yes | - | Bundle identifier of your app |
| `DATABASE_URL` | No | `sqlite:data.db` | SQLite database connection URL |

## Running the Server

```bash
export APNS_KEY_PATH=/path/to/AuthKey.p8
export APNS_KEY_ID=XXXXXXXXXX
export APNS_TEAM_ID=XXXXXXXXXX
export APNS_TOPIC=com.example.app

cargo run
```

The server listens on `0.0.0.0:3000`.
