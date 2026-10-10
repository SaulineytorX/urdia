# ADR 0047: Explicit browser Jev connection and OS credentials

Status: accepted for implementation; native vault acceptance scope remains limited

## Decision

The authenticated chat settings controller accepts strict Jev Connect/Check/
Restore/Disconnect/Forget requests. Browser Connect/Check/Restore require explicit
remote consent, a UUID and bounded keys. Only the official TypeSafe endpoint is
reachable from this connector; tests inject loopback only through cfg(test).
The fixed probe sends no project task. Jev remains advisory; it cannot alter
permissions, execution, defaults or automatic-routing gates.

Chat's local settings surface may accept this explicit user connection without
`--advisory-ui`. This does not load host adapters, environment keys, saved keys or
contact a remote service by default. That startup flag still controls preconfigured
host adapters; profile advice retains its separate flag. ADR0017's original
host-key-only browser setup is extended only for this consented connector.

Use OS storage with fixed per-user Urdia/Jev identity: Linux Secret Service or
Windows generic credential through WSL. No new Rust registry dependencies or
plaintext/config/browser-storage fallback. Helpers receive keys via bounded stdin,
not argv, have filtered environments, capped stdout, suppressed errors and five-
second deadlines. Explicit Restore reads the saved key; no background restoration.
Stored credentials are shared across project hosts under one OS user.

Disconnect releases only this session; Forget deletes the saved Urdia entry and
releases this host. Neither revokes the provider account key. Vault errors/timeout
must not claim save/deletion success; cancellation can leave an uncertain OS write.
Connection/test mutations serialize with advice and Laya. Reserve up to 16 UUID
receipts before side effects; repeated accepted IDs cannot repeat paid probes,
and changed payloads conflict. Cleanup remains usable at the limit.

## Verification and limits

Loopback HTTP tests cover typed authentication, explicit persistence, fresh-host
restore, forget, failed storage/probe, replay, conflicts and limits. Isolated CLI
helper fixtures verify stdin/argv separation, read/write/delete, missing backend
and bounded output. API tests cover auth, malformed requests, rejected arbitrary
endpoints and remote consent. Browser tests cover the form at desktop/mobile,
input/storage privacy, unchecked consent and explicit restore/delete through a
frontend fixture, plus the existing complete offline browser suite.

Actual Windows Credential Manager is not proven by fake stores. Linux GNOME
Secret Service now has explicit native acceptance through the isolated harness.
This server has neither an installed Windows bridge nor secret-tool;
its honest state is session-only. Do not claim native Windows or secret isolation
from same-user agents. Refresh release candidates after this runtime change;
previous contact/source/core artifacts do not contain this feature.

2026-10-08 follow-up: native libsecret reports quiet status1 on missing-item clear.
Treat only quiet missing read/clear as absence/idempotent deletion; cap/drain stderr
concurrently and return an error for diagnostic failures without echoing them.
Native tests exposed the repeated-delete bug and now verify the corrected behavior.
