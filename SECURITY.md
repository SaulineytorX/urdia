# Security policy

Urdia is an early Linux/WSL source preview, not a stable production release.

## Reporting

Report suspected vulnerabilities privately to
[saulineytor@mpcodersmx.com](mailto:saulineytor@mpcodersmx.com).
Include the affected version or commit, impact and minimal steps to reproduce.
Remove credentials, private access URLs and unrelated personal/project data from
attachments. Do not post vulnerability details in public issues, task prompts or
screenshots.

Report privately by email or through
[GitHub private vulnerability reporting](https://github.com/SaulineytorX/urdia/security/advisories/new).
No response time or fix deadline is promised for this preview.

## Current boundary

Urdia is a local single-user host, not a sandbox. Agents and explicitly approved
checks run as your operating-system user and can access files/network. Child agents
inherit the host environment; launch with only the credentials they need. Keep the
browser access URL private and listeners on loopback. Decisions are deny-by-default
with one-shot protocol permissions and host-validated receipt ordering. Optional
trusted scopes require separate, explicit activation and never come from AI advice.

No production signature or publisher trust identity accompanies this source
preview. Passing tests and checksums do not guarantee security.

## Jev keys in the workspace

The authenticated Jev form accepts an explicitly authorized, fixed connection
test. Optional Remember uses the host’s OS credential store, never a plaintext
project/browser/argv fallback. Restore is explicit after restart; the key is not
returned to the browser or injected into agent environments by this connector.
Same-user agents remain outside secret isolation. See
[Jev configuration](docs/jev-credentials.md) for shared-key scope, timeout ambiguity
and native-vault verification limits.

## Static result previews

Linux/WSL Conversation can explicitly capture trusted static workspace files and
open them on a separate fresh private localhost origin. The authenticated host
accepts no client path or command. It serves immutable bounded bytes, with an
exact Host capability and no control API, for at most 15 minutes. Browser code
runs when opened; the CSP is not complete network isolation or a security
sandbox. Stopping the link does not stop code already loaded in a tab. Close the
tab too. Do not share preview links or open untrusted project code. See
[the preview guide](docs/result-preview.md) and ADR0052.
