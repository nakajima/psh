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

### 3) Use the CLI in `psh-cli/`

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

## Curl API

The server has no auth. Put it behind a private network or proxy you trust.

```bash
export PSH=http://localhost:3000
```

### Health check

```bash
curl "$PSH/"
```

### Send a push

Plain curl bodies are treated as the notification body and sent to every registered device:

```bash
curl -X POST "$PSH/send" \
  --data 'hello from curl'
```

For APNs options, send JSON:

```bash
curl -X POST "$PSH/send" \
  -H 'Content-Type: application/json' \
  -d '{
    "title": "Hello",
    "body": "From curl",
    "sound": "default",
    "badge": 1,
    "interruption_level": "time-sensitive",
    "data": {"url": "psh://example"}
  }'
```

`/send` accepts these JSON fields:

- alert: `title`, `subtitle`, `body`, `launch_image`
- localization: `title_loc_key`, `title_loc_args`, `loc_key`, `loc_args`
- badge/sound: `badge`, `sound` (`"default"` or `{ "name": "alert.caf", "critical": true, "volume": 0.8 }`)
- behavior: `content_available`, `mutable_content`, `category`, `interruption_level`, `relevance_score`
- delivery: `priority` (1-5 normal, 6+ high), `collapse_id`, `expiration` (Unix timestamp)
- custom payload keys: `data` object

Response:

```json
{
  "success": true,
  "sent": 1,
  "failed": 0,
  "results": [
    {
      "device_token": "...",
      "success": true,
      "apns_id": "...",
      "error": null
    }
  ]
}
```

### Register a device

The app normally calls this after APNs registration, but it can be called directly:

```bash
curl -X POST "$PSH/register" \
  -H 'Content-Type: application/json' \
  -d '{
    "device_token": "apns-device-token",
    "installation_id": "device-installation-uuid",
    "environment": "sandbox",
    "device_name": "My iPhone",
    "device_type": "iPhone",
    "os_version": "iOS 18.0",
    "app_version": "1.0"
  }'
```

Required fields are `device_token`, `installation_id`, and `environment` (`sandbox` or `production`).

### Stats

```bash
curl "$PSH/stats"
```

```json
{
  "total_devices": 1,
  "sandbox_devices": 1,
  "production_devices": 0,
  "total_pushes": 12
}
```

### Push history

```bash
curl "$PSH/pushes?installation_id=device-installation-uuid"
curl "$PSH/pushes/1"
```

`GET /pushes?installation_id=...` returns `{ "pushes": [...] }`. `GET /pushes/:id` returns one detailed push record.

## Development Commands

```bash
# Rust tests
cargo test --manifest-path server/Cargo.toml
cargo test --manifest-path psh-cli/Cargo.toml

# iOS tests (fastlane)
bundle exec fastlane ios test
```
