# Run your own OM Home with SQLite

This Home serves OM mailboxes, discovery and first contact, key material, wake sockets, temporary encrypted blobs and the recovery store. It runs on a machine you control and uses local SQLite storage. The app's connected wake channel works over HTTPS. Browser push registration is disabled.

## Start from the source tree

Use Node.js 24.14 or later in the 24 line and the pnpm version in the workspace manifest. From the repository root, install with `pnpm install --frozen-lockfile`. Copy the **setup value** shown by OM when preparing a delivery service. Keep the setup screen open so the same app retains the secret that matches it. The setup value is a 64-character hash.

```sh
OM_ADMISSION='<your OM setup value>' OM_DATA_DIR="$HOME/.local/share/om-home" pnpm --filter @om/sqlite-home start
```

The server creates the directory if needed and listens on `127.0.0.1:8787`. Run it under an unprivileged account with exclusive access to the data directory. The directory must remain available across restarts. Run one instance per directory. Requests to create resources remain closed until the server has a valid setup value. Peers use the capabilities the app gives them. To accept setup values from additional devices, supply their comma-separated hashes in `OM_ADMISSION` and restart.

## Run with Docker

From the repository root, build the image once and run it with a persistent volume.

```sh
docker build -f infra/sqlite-home/Dockerfile -t om-sqlite-home .
docker run -d --name om-home --restart unless-stopped \
  -p 127.0.0.1:8787:8787 -v om-home-data:/var/lib/om-home \
  -e OM_ADMISSION='<your OM setup value>' om-sqlite-home
```

## Run under systemd

For a machine that keeps the Home running, install the built bundle and the unit in `deploy/`. Build once from the repository root (`pnpm --filter @om/sqlite-home build`), then as root:

```sh
install -d /opt/om-home /etc/om-home
install -m 0644 infra/sqlite-home/dist/home.mjs /opt/om-home/home.mjs
install -m 0600 infra/sqlite-home/deploy/om-home.env.example /etc/om-home/om-home.env
install -m 0644 infra/sqlite-home/deploy/om-home.service /etc/systemd/system/om-home.service
```

Edit `/etc/om-home/om-home.env` and set `OM_ADMISSION` to your OM setup value, then start it.

```sh
systemctl daemon-reload
systemctl enable --now om-home.service
systemctl status om-home.service
```

The unit runs the Home as a throwaway unprivileged account, keeps its data in `/var/lib/om-home` (mode 0700), restarts it after a crash, stops it with a clean close, and removes access to the rest of the system. It expects `node` at `/usr/bin/node` (Node.js 24.14 or later in the 24 line); change `ExecStart` if yours is elsewhere. Run `systemd-analyze security om-home.service` to see the sandbox as your systemd reports it. Take a consistent copy of the data directory only with the service stopped (`systemctl stop om-home.service`).

## Give the Home an HTTPS address

Point a hostname you control at the machine that runs the HTTPS proxy. On the same machine as the Home, configure Caddy with the following site block and replace the example hostname. Allow external connections to the proxy on ports 80 and 443. Keep port 8787 private. Caddy obtains the certificate and forwards the WebSocket upgrade. [Caddy automatic HTTPS](https://caddyserver.com/docs/automatic-https) and [Caddy reverse proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy).

```caddy
om-home.example.org {
    reverse_proxy 127.0.0.1:8787
}
```

Keep request and debug logging disabled for the Home at the proxy and any intermediaries. Do not cache private responses, rewrite paths or redirect authenticated requests to another origin. Other proxies must forward WebSocket upgrades, the authorization and admission headers, conditional request headers and binary request bodies.

Check that the address serves capabilities, then enter the HTTPS address on the setup screen in OM.

```sh
curl --fail --silent https://om-home.example.org/.well-known/om-provider-capabilities
```

Look for `om.admission.v1`, `om.relay.wake.v1`, `om.blob.spool.v1` and `om.recovery.store.v1` in `features`.

## Storage and limits

- The `personal` profile is the default. `OM_PROFILE=default` selects the reference Home's larger limits and disables the usage meter. Use `OM_WAKE=0`, `OM_BLOB_SPOOL=0` or `OM_RECOVERY_STORE=0` to disable an optional feature.
- The personal usage meter retains the Home API's `worker_requests` and `durable_object_requests` field names. On this Home they are logical ingress and storage-operation units. They have no billing effect. At the hard threshold the Home sheds new queue creation, mailbox deposits, introduction requests and blob puts. Reads, drains and acknowledgements remain available.
- Store the Home on local persistent storage and stop it before making a consistent copy of the complete data directory. Restoring an older copy can omit accepted or acknowledged items. The application's end-to-end history and identity are separate from this data.
- The data directory contains encrypted payloads, opaque identifiers, capability hashes, and the state needed for idempotent retries. Roles share one operator and keep separate SQLite files. Restrict access to the directory and any copies. The Home does not write request logs.

Capacity measurements and the budget one Home needs for a number of people are in [`2026-11-22-sqlite-home-real-host-and-capacity.md`](../../docs/reports/2026-11-22-sqlite-home-real-host-and-capacity.md).

The source build, Dockerfile and systemd unit are for staging and self-hosted evaluation. Release-integrity and provider-diversity gates are tracked separately. Run the shared cases with `pnpm conformance:provider` from the repository root.
