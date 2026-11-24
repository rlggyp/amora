# Amora

Amora (AlertManager to Grafana) is a webhook-to-SSE bridge. It receives alert notifications from Prometheus Alertmanager and pushes them to Grafana or any SSE client in real time.

---

## How It Works

```
Alertmanager ──(POST /api/webhook)──► Amora ──(GET /api/events)──► Grafana / Web Client
```

---

## Endpoints

| Method | Path | Auth | Description |
|---|---|---|---|
| `POST` | `[/<base_path>]/api/webhook` | `Bearer <token>` | Webhook receiver for Alertmanager |
| `GET` | `[/<base_path>]/api/events` | None | Real-time SSE stream for clients |

---

## Quick Start

### Docker

```bash
docker run -d \
  --name amora \
  -p 12000:12000 \
  -e AMORA_BEARER_TOKEN="secret-token" \
  rlggyp/amora:latest
```

Or with Docker Compose:

```yaml
services:
  amora:
    image: rlggyp/amora:latest
    container_name: amora
    restart: unless-stopped
    ports:
      - "12000:12000"
    environment:
      - PORT=12000
      - AMORA_BEARER_TOKEN=secret-token
      # - AMORA_BASE_PATH=amora
      # - AMORA_KEEP_ALIVE_INTERVAL=15
```

### Local Build

```bash
export AMORA_BEARER_TOKEN="secret-token"
cargo run
```

---

## Configuration

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `PORT` | `12000` | HTTP listening port |
| `AMORA_BEARER_TOKEN` | *(None)* | Bearer token required for `POST /api/webhook` |
| `AMORA_BASE_PATH` | *(None)* | Optional base path prefix (e.g. `amora` mounts to `/amora/api/...`) |
| `AMORA_KEEP_ALIVE_INTERVAL` | `15` | SSE keep-alive interval in seconds |
| `CONFIG_FILE` | `configs/config.yaml` | Path to YAML config file (optional, for CORS) |
| `RUST_LOG` | `info` | Log level filter |

### CORS Configuration (`configs/config.yaml`)

```yaml
bearer_token: "secret-token"
cors:
  allow_credentials: true
  allow_origins:
    - http://localhost:3000
    - http://localhost:5173
  allow_methods:
    - GET
    - POST
  allow_headers:
    - content-type
    - x-real-ip
    - authorization
```

---

## Alertmanager Configuration

Add Amora as a receiver in your `alertmanager.yaml`:

```yaml
receivers:
  - name: 'amora'
    webhook_configs:
      - url: 'http://<amora-host>:12000/api/webhook'
        http_config:
          authorization:
            credentials: 'secret-token'
```

---

## Client Usage (SSE)

Connect via curl:
```bash
curl -N http://localhost:12000/api/events
```

Or in JavaScript:
```javascript
const es = new EventSource("http://localhost:12000/api/events");
es.onmessage = (event) => {
  const payload = JSON.parse(event.data);
  console.log("Alerts:", payload.alerts);
};
```
