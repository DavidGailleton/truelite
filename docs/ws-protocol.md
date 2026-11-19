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

## Actions

Actions change something on the NAS. Each one is confirmed in a dialog in the web app, and the
server checks its params strictly and rebuilds them from the checked values (bad ones get
`bad_request`; options the web app never sends are refused, not dropped). There are two kinds.

### Jobs

TrueNAS jobs (`JOB_METHODS` in `server/src/allowlist.rs`). The app jobs need `APPS_WRITE`:

- `app.start`, `app.stop`, `app.redeploy` (restart): `[app_name]`.
- `app.upgrade`: `[app_name]` or `[app_name, {"snapshot_hostpaths": bool}]`; sent as
  `[app_name, {"app_version": "latest", "snapshot_hostpaths": …}]`, never with `values`.
- `app.pull_images`: `[app_name]`; sent as `[app_name, {"redeploy": true}]`.
- `pool.scrub.scrub` (`POOL_WRITE`): `[pool]` or `[pool, "START"|"PAUSE"|"STOP"]`; sent as
  `[pool, action]` (START when absent, which also resumes a paused scrub). The pool name is a
  ZFS pool name (a letter, then letters, digits, `_ - . :`). START's job lasts the **whole
  scrub** (hours) and ends when it finishes, pauses or is stopped; follow the scrub through the
  `pool.scan` event rather than this reply. `pool.scrub.run` is not allowed: it does nothing
  unless a scrub is due.
- `update.run` (`SYSTEM_UPDATE_WRITE`): `[{"train": t, "version": v, "reboot": bool, "resume":
  bool}]` (`resume` optional, default false); sent with exactly these keys, never
  `dataset_name`. `train` and `version` are required (letters, digits, `. _ -`, at most 64
  characters), from `update.status`, so the NAS installs the version the user saw. Progress:
  download 0–50 %, install 50–100 % (no final 100 %). With `reboot` the NAS restarts about 10 s
  after the reply. A pre-check warning fails the job with `errname` `EAGAIN` and the warning as
  the message: call again with `resume: true` to accept it. Without `reboot`, `update.status`
  then reports `code: ERROR`, `errname` `EREBOOTREQUIRED`. `update.download` is not allowed.
- `system.reboot`, `system.shutdown` (`FULL_ADMIN`): `[]`; sent as `["Requested from truelite",
  {"delay": null}]` (the reason in the NAS audit log is truelite's, never the browser's). The
  job runs the restart or power-off itself, so the reply may never come: the link drops
  (`reconnecting`, then `logged_out` once the NAS is back, as its tokens are gone). Treat a
  `result` and a dropped link alike as "the NAS is going down".

The server follows the job on the NAS and sends its progress under the call's `id` until the
reply:

```json
→ {"op": "call", "id": 7, "method": "app.stop", "params": ["plex"]}
← {"id": 7, "progress": {"percent": 40, "description": "Stopping containers"}}
← {"id": 7, "result": null}
```

- `percent` is 0 to 100 or `null`; `description` is the NAS's text (at most 200 characters)
  or `null`. Progress comes only when it changes, at most every 250 ms, and may be skipped:
  show the latest one.
- There is no timeout: a job may take minutes (pulling images). The reply comes when the job
  ends: `result`, or `error` with code `nas` when the job failed or the role is missing
  (`errname` `EACCES`).
- The job goes on on the NAS when the page or the socket closes, and after `reconnecting`: its
  outcome then shows in the `app.query` events.

### Plain actions

Methods that answer at once (`ACTIONS` in `server/src/allowlist.rs`): after the params check
they go out like any call, with its timeout and no progress.

- `pool.snapshot.create` (`SNAPSHOT_WRITE`): `[{"dataset": path, "name": name, "recursive":
  bool}]` (`recursive` optional, default false). No naming schema, exclude list, properties or
  VMware sync. The result is the new snapshot, cut down like `pool.snapshot.query`.
- `pool.snapshot.delete` (`SNAPSHOT_DELETE`): `["dataset@name"]`; sent as `[id, {}]`: one
  snapshot, never recursive or deferred. The result is `true`.

Dataset paths and snapshot names must be ZFS names: letters, digits, `_ - : .` and inner
spaces; names at most 200 characters.

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

### Topics with arguments

Some TrueNAS events take arguments, written after the name as JSON: `name:{…}`. The server
allows only the ones listed in `PARAM_EVENTS` (`server/src/allowlist.rs`), checks every argument
with a strict schema (no unknown or missing keys) and subscribes on the NAS under a name it
rebuilds itself; the browser's string is never forwarded. Messages come back under the
browser's own `topic` string, so `unsub` takes that same string. Today the only one is
`app.container_log_follow` (below); `reporting.realtime:{"interval": …}` and the like are
refused with `not_allowed`.

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

**`pool.query`**: pool changes (needs `POOL_READ`). TrueNAS sends the whole pool with its disk
layout (`topology`) on every change; the server keeps only the fields the web app selects in its
own `pool.query` call: `id`, `name`, `status`, `healthy`, `warning`, `status_code`,
`status_detail`, `size`, `allocated`, `free`, `scan` (the list is `FIELDS` in
`server/src/truenas/pools.rs`). A `removed` event has only its `id`.

The `pool.query` **call** passes through as the NAS sends it, except `topology` when the call
selects it (the pools page does): each of its six groups (`data`, `log`, `cache`, `spare`,
`special`, `dedup`) keeps, per node, `name`, `type`, `status`, `disk` (disks only),
`errors: {read, write, checksum}` and `children` (same shape, nested at most 6 deep). `stats`,
`path` and `guid` are dropped. `topology` is `null` when TrueNAS has none.

`pool.scan` (scrub and resilver progress, `POOL_SCRUB_READ`) is small and forwarded as it is:
`fields` = `{"name": "tank", "scan": {…}}`, the same `scan` object as in `pool.query`.

**`pool.dataset.query`**: datasets created, changed or deleted (needs `DATASET_READ`). Space
changes send no event. The call result and each event's `fields` are cut down the same way
(`server/src/truenas/datasets.rs`): `id`, `type`, `encrypted`, `locked`, `key_loaded` as sent,
and `used`, `available`, `quota`, `refquota` as plain byte counts. TrueNAS sends those four as
`{parsed, rawvalue, …}` objects; the server keeps `parsed`, or `rawvalue` read as an integer, or
leaves the field out. Everything else (`children`, `user_properties`, `mountpoint`, other
properties) is dropped. A `removed` event has only its `id`.

**`pool.snapshot.query`**: snapshots taken or deleted through the API (needs `SNAPSHOT_READ`).
Periodic snapshot tasks send no event: read the list again. Events cover every snapshot of the
NAS; the web app keeps those of the dataset it shows. The call result and each event's `fields`
are cut down the same way (`server/src/truenas/snapshots.rs`): `id`, `dataset`,
`snapshot_name` as sent, `txg` (`createtxg` as a number), `used` and `referenced` as byte counts
and `created` as seconds since the epoch (from the `creation` property), each left out when
missing. Ask for those properties with `extra.properties`. A `removed` event has only its `id`.

**`disk.query`, `disk.temperatures`** (call results only; the disks page). TrueNAS sends no
`disk.query` events and has no temperature event, so the page reads both again. Each disk of a
`disk.query` result keeps `identifier`, `name`, `serial`, `model`, `size`, `type`,
`rotationrate`, `bus`, `pool`, `description` (`FIELDS` in `server/src/truenas/disks.rs`):
`passwd` and `kmip_uid` never reach the browser. Ask with `extra.pools: true`, or `pool` is
always `null`. `disk.temperatures` (needs `REPORTING_READ`; call it with
`[[], true]` for the critical limits) comes back as `{name: {temp, crit}}`, both °C or `null`,
whether TrueNAS sent `n` or `[n, crit]`. `disk.temperature_agg` (`[names, days ≤ 7]`) is
forwarded as sent: `{name: {min, max, avg}}`. SMART is not in the TrueNAS 25.10 API.

**`app.query`**: app changes (needs `APPS_READ`). TrueNAS sends the whole app on every change:
its containers, ports and volumes (`active_workloads`), `portals`, the catalog `metadata` and
`notes`. The server keeps only the fields the web app selects in its own `app.query` call: `id`,
`name`, `state`, `upgrade_available`, `image_updates_available`, `custom_app`, `version`,
`human_version` (the list is `FIELDS` in `server/src/truenas/apps.rs`). App ids are strings (the
app name). A `removed` event has only its `id`. The app detail page needs the workloads, so it
queries its one app again on each event about it (`app.query` call results are not trimmed).

**`alert.list`**: alert changes (needs `ALERT_LIST_READ`). Each alert carries its class
arguments (`args`) and its e-mail state (`mail`), and the `alert.list` method takes no `select`,
so the server trims **both the events and the `alert.list` call result** to: `uuid`, `id`,
`klass`, `level`, `formatted`, `text`, `datetime`, `last_occurrence`, `dismissed`, `one_shot`
(the list is `FIELDS` in `server/src/truenas/alerts.rs`). The list includes dismissed alerts.
`formatted` may contain HTML: show it as text, never as markup. A `removed` event has only its
`id`.

**`update.status`**: whether a TrueNAS update is available (needs `SYSTEM_UPDATE_READ`).
TrueNAS sends the new version's `manifest` (checksums, file list), so the server trims **both
the events and the call result** to the same flat status, whether the event wraps it in
`{status: …}` or not:

```json
{"code": "NORMAL",
 "status": {"current_version": {"train": "TrueNAS-SCALE-Goldeye", "profile": "GENERAL",
                                "matches_profile": true},
            "new_version": {"version": "25.10.9", "release_notes": "…",
                            "release_notes_url": "https://…"}},
 "error": null, "update_download_progress": null}
```

`new_version` is `null` when TrueNAS is up to date. With `code: "ERROR"`, `status` is `null` and
`error` is `{errname, reason}`. Events are only `changed` and carry no `id`.

**`app.container_log_follow:{"app_name", "container_id", "tail_lines"}`**: one container's log
(needs `APPS_READ`). `app_name` is the app id (lowercase letters, digits, dashes),
`container_id` the container's `id` from `app.query` (`active_workloads.container_details`, 64 or
12 hex digits), `tail_lines` the lines of history to send first, 1 to 200 (a whole log is never
sent). Key order and spaces do not matter.

```json
→ {"op": "sub", "topic": "app.container_log_follow:{\"app_name\":\"plex\",\"container_id\":\"5c3e…\",\"tail_lines\":200}"}
← {"topic": "<same string>", "status": "subscribed"}
← {"topic": "<same string>", "data": {"kind": "added", "id": null, "fields": {"lines": [
     {"time": "2026-11-18 10:00:01+00:00", "text": "Server started"}]}}}
```

- Lines come in batches: every line waiting on the server goes in one message, up to 100 lines
  or 32 KiB of text. `time` is the Docker timestamp as TrueNAS formats it, or `null`; `text` has
  no trailing newline and is cut at 8 KiB (ending with `…`).
- There is nothing to query. The tail arrives as the first batches after `subscribed`.
- `resumed` (after a NAS reconnect) and a new subscription send the tail again: clear the page's
  lines first. `missed`: some lines were skipped.
- `ended`: the app is stopped (refused with `nas`, "Unable to retrieve logs of stopped … app"),
  the container is gone, or the container stopped (the NAS ends the stream). A restarted app may
  have new container ids: query the app and subscribe again.
- Two tabs of the same login following the same container share one subscription on the NAS:
  the second one gets new lines only, not the tail.

## Link and session

- `{"op": "link", "state": "reconnecting"}`: the NAS connection dropped; the server is
  reconnecting. Show "Reconnecting to NAS"; calls made now wait for the link.
- `{"op": "link", "state": "up"}`: back. Topics follow with `resumed`.
- Close code **4401** ("log in again"): the session ended (logout in another tab, idle or
  maximum age, the NAS no longer accepts it). Go to the login page; do not reconnect.
- Any other close: reconnect with backoff; the session may still be alive.

The server pings every 30 s; browsers answer on their own.
