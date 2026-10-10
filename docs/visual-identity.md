# Urdia visual identity

Urdia's Portal mark uses an angular U, a lifted route segment and a central
control point. The owner selected a futuristic, energetic direction. Cyan, mint
and violet give the symbol more presence while its solid silhouette remains
recognizable at icon sizes. This is original project artwork, not a provider logo.
The canonical SVG supersedes the first rounded routing-U concept.

## Assets and use

The canonical [SVG mark](../crates/urdia-ui/assets/brand.svg) is a scalable,
self-contained vector with no scripts, remote resources or embedded fonts. It
ships inside the binary and is served locally at `/brand.svg` with an SVG MIME
type and the existing security headers. The same asset appears in the sidebar,
the locked screen, browser favicon and repository README. It contains no session
metadata and can be retrieved without authentication, like the other static
interface assets. Protected APIs still require their bearer token.

- Dashboard background: `#101315`; icon tile gradient: `#1b2340` to `#0c111b`.
- Symbol gradient: mint `#88ffd7`, cyan `#65cfff`, violet `#b4a1ff`.
- Primary actions remain mint `#b5efc0`; their semantic roles do not change.
- Supporting dashboard blue remains `#9ba9ff`.
- Foreground: `#eef1ec`; muted text: `#a1aaa6`.
- Typography: the existing system sans-serif stack, with no font download.

Keep the square aspect ratio and use at least 16 pixels for the icon. Leave clear
space around it; do not distort the symbol or use its color accents to suggest an
agent connection was verified. Runtime state must still use explicit text.
When placed beside the Urdia name, the image is decorative for screen readers;
the locked screen names it. Browser tab support for SVG favicons depends on the
browser. No PNG/ICO fallback or native desktop icon is supplied in this slice.

The dashboard credits **SaulineytorX** as creator. Source artwork follows the
repository's Apache-2.0 license; this credit does not add a new attribution or
usage restriction.

Portal refines the sidebar and header accents, selected navigation and wordmark
spacing. The conversation now includes a branded empty state, separate message
surfaces and a responsive strip of five observed metrics. Cyan highlights running
work, amber marks pending permission/stopping, mint marks completed turns and
coral marks failure/cancellation; explicit labels accompany every state.

This presentation does not change task, routing, credential or permission
behavior. There are no animated effects or external resources. Reduced-motion
preferences disable button transitions.
