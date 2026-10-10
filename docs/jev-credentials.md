# Configure Jev in Urdia

Open **Decision engines → Jev**, paste your API key, and authorize the small
connection test. Choose **Connect and test**. The key input clears immediately;
the response contains only connection/storage metadata. Testing sends a fixed
English connection check to the official TypeSafe endpoint, not your project
objective or files. The pinned adapter may retry retryable failures; this can
consume provider quota. Connection success does not verify recommendation quality.

No terminal key entry or restart is needed to connect. The authenticated chat
settings surface offers this connector even when no host decision adapters were
loaded at startup. Without your explicit remote-test consent it makes no Jev call.
Requesting advice later requires separate remote consent and never starts a task or
grants permissions. Manual mode remains default. The owner-requested [automatic Jev preview](automatic-jev.md)
can dispatch one explicitly submitted task; ADR0015 production-quality evidence
has not passed. Selecting that mode is separate from key connection.

## Remember and use a saved key

**Remember in this computer's credential store** is optional and unchecked by
default. When available it uses:

- Linux: the system Secret Service keyring through `/usr/bin/secret-tool`, with
  a local Unix session bus. The keyring may need to be unlocked.
- WSL: the Windows Credential Manager through the standard System32 Windows
  PowerShell bridge when WSL interoperability and the default Windows mount exist.
  Native Windows Urdia is not implied. Actual Windows vault acceptance remains
  pending. Native GNOME Secret Service on Linux is now verified in a disposable
  home/session, in addition to isolated protocol/state tests.

Urdia does not install a vault or write plaintext fallback credentials. Without a
usable system store, Remember is disabled; connect without it for this session.
Availability means the OS bridge exists, not that the vault is unlocked or a saved
key exists. Vault failures are redacted and never silently claim a successful save.
Windows generic credential storage has a smaller key-size limit than session
connections; an oversized remembered key is refused before a paid probe.

After restarting Urdia, choose **Use saved key** and authorize its connection test.
There is no automatic vault read or paid connection test on page load/restart.
Keys are stored on the machine running the Urdia host, which can differ from
the machine displaying its browser UI. Saved keys are shared across Urdia projects under your OS account; the latest
successful save is used by later restores. Session-only replacement leaves an
older saved key unchanged. A save records success at that moment; another running
host can subsequently replace the shared saved key.

- **Disconnect session** drops this host's connection; the saved key stays.
- **Forget saved key** removes only Urdia's Jev vault entry and disconnects this
  host. It does not revoke the TypeSafe account key or disconnect other hosts.
- **Test connection** makes another explicitly authorized fixed probe.

If a save times out, its OS effect can be uncertain. Check the OS store rather
than assuming the old entry is unchanged. Connection retries with the same
request ID do not repeat an admitted probe within this host; at most 16 unique
connection tests are admitted. Disconnect/Forget remain usable after the limit.

## Privacy boundary

Keys are never returned through an API view, included in the task/event database,
written to browser storage/project configuration, placed in command arguments, or
exported into agent environments by this connector. They live in host memory and,
only with Remember, the OS store. Password inputs clear on submission, auth lock
and page hide. This does not promise secure erasure or isolation from another
process/agent running under the same OS account. Agents already inheriting an
externally configured host environment remain subject to that existing boundary.

Existing `TYPESAFE_API_KEY`/systemd credential setup remains supported for CLI
routing and explicit host-configured advisory startup. No environment key is
saved automatically. Disconnect/Forget do not edit those external credentials.

Contract references: [TypeSafe API](https://docs.typesafe.ai/api),
[Secret Service CLI](https://manpages.debian.org/bookworm/libsecret-tools/secret-tool.1.en.html),
[Windows credential write](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew).

## Optional native Linux acceptance

When the native test tools already exist, run from the source root:

```sh
python3 -I -B scripts/check-native-secret-service.py \
  --output "$HOME/urdia-native-vault-evidence.json"
```

Output must be new and outside the checkout. The harness creates its own disposable
home, random nonempty vault password, private Unix D-Bus socket and synthetic key.
It verifies save/read, unreadability with an incorrect password, recovery after
service restart, deletion twice and deletion remaining effective after another
restart. It also checks the synthetic key's bytes are absent from native persisted
keyring files; this is not a cryptographic audit. No real provider key/model is used.
The Rust native test is intentionally ignored by ordinary workspace checks and
requires the harness's isolation guards when explicitly run.

Optional `--secret-tool`, `--daemon` and `--library-path` select trusted test tools
extracted into temporary directories; the harness installs nothing and leaves the
host's normal vault/services unchanged. Sanitized JSON evidence is retained; private
vault files and raw logs are removed. Linux results apply to the tested GNOME/
libsecret versions, not every Secret Service backend or a Windows credential store.

The optional native harness also works in an extracted source snapshot without
Git. Its evidence then records an unversioned source, null commit/clean fields
and the tested credential-store file hash; it does not infer the identity of a
surrounding Git repository. Bind that result to your archive checksum separately.
