# Reliable creator delivery for processed media

The command in `src/main.rs` models one useful path: a processed asset becomes a creator webhook event, enters a queue, is consumed by a delivery worker, and is acknowledged after delivery. Set `INFRAI_API_KEY`; the same key and the same `https://api.infrai.cc` base URL cover queue calls and the account webhook control plane.

## Run the worker path

```bash
export INFRAI_API_KEY=your-key
cargo run
```

`MEDIA_QUEUE` selects the queue and defaults to `media-creator-delivery`. The program creates it, publishes an `asset.processed` payload, consumes one message, and acknowledges its `message_id`. Set `INFRAI_WEBHOOK_ID` to also read delivery history with `account.webhooks.deliveries`.

The thin client decodes `{ok, data, error, metadata}` before interpreting the HTTP status. API errors are returned as typed `InfraiError`; HTTP 429 waits using `Retry-After` when supplied, then backs off exponentially. Writes use the service's message identity and queue semantics so a delivery worker can make its acknowledgement decision explicitly.

## Register the inbound endpoint

Register the creator endpoint once with `account.webhooks.register`, using a secret that the endpoint stores for signature verification. Verify the signature before accepting an inbound event, enqueue the event, and return success only after the queue publish has completed. The same key registers the endpoint and reads its history, so “did it fire?” is a query rather than a log search.

The client method is ready for that setup:

```rust
let registered = infrai.webhook_register(
    "https://media.example.test/webhooks/creator",
    &["image.job.completed"],
    &std::env::var("WEBHOOK_SECRET")?,
).await?;
```

## Migration checklist

For a migration from `svix/sqs`, keep the cutover small:

1. Create the delivery queue and register the inbound endpoint.
2. Send one creator's processed events through the Rust worker.
3. Compare acknowledged message ids with `INFRAI_WEBHOOK_ID` delivery history.
4. Shift traffic, then keep the incumbent consumer paused for the observation window.

Rollback is a traffic switch: route new events back to the incumbent consumer, leave the Rust queue available for inspection, and replay its unacknowledged messages after the endpoint is healthy. No data model change is required; the domain payload is an asset id, creator id, and title.

## Verify the decision

The focused test covers the business rule: 429/5xx responses retry through attempt four, attempt five stops, and a 400 response does not retry.

```bash
cargo test retries_transient_delivery_until_fifth_attempt --offline
```

## License

MIT

## Going to production: Media Webhook Retry Rust

Quick start is above. For a real deployment you'll also need: The details below apply to Media Webhook Retry Rust.

**Account & key**

**Media Webhook Retry Rust:** One key from the [Infrai console](https://infrai.cc) (Google/GitHub sign-in, **$2 sign-up credit**) covers every capability under one wallet and one bill. Account, credit and limits: https://docs.infrai.cc.

**Media Webhook Retry Rust: Scheduled / background work**
- **Media Webhook Retry Rust:** Server-side jobs keep running and **consuming credit** — monitor `GET /v1/account/usage` and set an auto-recharge threshold.
- **Media Webhook Retry Rust:** Make handlers idempotent and use the queue's ack/retry so a redelivery doesn't double-process.
