# Browser ↔ server WebSocket protocol

The web app talks to the NAS only through `GET /ws` on the truelite server. It is our own small
protocol, not raw JSON-RPC: the server forwards only the methods and events on its allowlist
(`server/src/allowlist.rs`). Calls run as the logged-in TrueNAS user, so TrueNAS roles still
apply. Every message is one JSON text frame of at most 64 KiB; anything larger closes the socket.

## Opening

Open `/ws` with the session cookie (set by `POST /api/login`), from the app's own page: the
`Origin` the browser sends must match the address the request goes to (or be listed in
`TRUELITE_ALLOWED_ORIGINS`), otherwise the upgrade is refused with `403` before the session is
looked at. There is no cookie or session → the upgrade is refused with `401`; go to the login
page. The first message is always the link
state:

```json
{"op": "link", "state": "up"}
```

## Calls

```json
→ {"op": "call", "id": 1, "method": "system.info", "params": []}
← {"id": 1, "result": {"hostname": "truenas", "...": "..."}}
← {"id": 1, "error": {"code": "not_allowed", "message": "truelite does not allow this action"}}
```

- `id` is a number from 0 to 2³²−1, chosen by the browser; replies come back as they finish,
  not in order. `params` is a list (positional, as TrueNAS takes them) and may be left out.
- A message that is not valid gets `bad_request`, with `"id": null` when no id could be read.
  The socket stays open.

Error `code`s, with `message` always in plain words:

| code | meaning | what the app does |
| --- | --- | --- |
| `not_allowed` | method or event not on the allowlist | a bug in the app |
| `bad_request` | malformed message | a bug in the app |
| `nas` | the NAS refused; `errname` (`EACCES`, …) when it gave one | show `message` |
| `timeout` | the NAS did not answer in time | offer a retry |
| `reconnecting` | the link to the NAS did not come back in time | offer a retry |
| `busy` | too many calls or topics at once on this socket | retry later |
| `logged_out` | the session has ended | the socket closes with 4401 right after |

A call is never sent twice: after a `timeout` or `reconnecting` it may or may not have run.

## Subscriptions

```json
→ {"op": "sub", "topic": "alert.list"}
← {"topic": "alert.list", "status": "subscribed"}
← {"topic": "alert.list", "data": {"kind": "added", "id": 7, "fields": {"...": "..."}}}
→ {"op": "unsub", "topic": "alert.list"}
```

- Wait for `subscribed`, then query the current state (`alert.list` call): changes after
  that point arrive as `data`. `kind` is `added`, `changed`, `removed` or `other`.
- Subscribing twice to a topic on one socket does nothing. `unsub` of an unknown topic is
  ignored. Closing the socket unsubscribes everything.
- `{"topic": …, "status": "resumed"}`: subscribed again after a reconnect. Events of the gap
  are lost: query the state again.
- `{"topic": …, "status": "missed", "count": n}`: the page read too slowly. Query again.
- `{"topic": …, "status": "ended", "error": {…}}`: refused (`not_allowed`, `nas`, `busy`) or
  ended by the NAS. Nothing follows; subscribe again to retry.

## Reshaped topics

Most topics forward the NAS event's `fields` as they are. Some streams are large and mostly
unused, so the server sends only what the web app shows:

**`reporting.realtime`**: live statistics every 2 s (needs the `REPORTING_READ` role). There is
nothing to query: each message is a full sample, and `kind` is whatever the NAS sent. A sample
the server cannot read is skipped. `fields`:

```json
{"cpu": 12.3, "temp": 47.5,
 "mem": {"total": 68719476736, "available": 30000000000, "arc": 25000000000},
 "net": {"enp3s0": {"rx": 1250, "tx": 360, "up": true, "speed": 1000}}}
```

| field | unit |
| --- | --- |
| `cpu` | whole-CPU usage, %, one decimal |
| `temp` | CPU temperature in °C (the hottest core when there is no overall value), or `null` |
| `mem.total`, `mem.available`, `mem.arc` | bytes: physical memory, memory available, ZFS cache (ARC) size |
| `net.<name>.rx`, `.tx` | bytes per second received and sent |
| `net.<name>.up` | link up |
| `net.<name>.speed` | link speed in Mb/s, or `null` when unknown |

`net` lists physical interfaces and bridges, without loopback and container or VM plumbing
(`veth*`, `docker*`, `br-*`, `vnet*`, `tap*`, …).

## Link and session

- `{"op": "link", "state": "reconnecting"}`: the NAS connection dropped; the server is
  reconnecting. Show "Reconnecting to NAS"; calls made now wait for the link.
- `{"op": "link", "state": "up"}`: back. Topics follow with `resumed`.
- Close code **4401** ("log in again"): the session ended (logout in another tab, idle or
  maximum age, the NAS no longer accepts it). Go to the login page; do not reconnect.
- Any other close: reconnect with backoff; the session may still be alive.

The server pings every 30 s; browsers answer on their own.
