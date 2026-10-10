# ADR 0041: Explicit original-agent reconnection

- Status: accepted
- Date: 2026-10-06 UTC
- Extends: ADR 0039; preserves ADR 0015 and ADR 0040.

## Decision

Expose authenticated, strict POST /api/conversation/reconnect only as an explicit
revision-bound request with confirm_context_reset=true. Enable recovery for a
failed original launch in non-trusted hosts, not for separate fresh-profile
failures. Reconnection is mutually exclusive with task admission, verification,
profile selection and plan mutation; duplicate pending delivery is idempotent.
Pending permission requests or any configured trusted broker reject it in both
API and host. Host drains finished tasks, releases the original session and closes
its lifecycle as replaced before starting one new session. Use the same immutable
provider/workspace/requested selectors and receipt sink/broker. No prompt,
transcript, automatic retry, fallback or permission is issued by recovery.

Startup is interruptible with host shutdown and bounded to 60 seconds, in addition
to adapter deadlines. Failures retain only ADR0039's static category text. After
failed startup the original session is unavailable: reject its new task admission,
allow an explicitly selected fresh profile or renewed reconnect consent. Limit
attempts to16 per UI process; stale results/requests cannot replace a later state.

Host plan, verification evidence, bounded turns and browser composer survive.
Freeze original-turn execution identity at admission, so new startup observations
do not relabel old tasks. Current startup/live observations reset for the new
session. History includes old closure/new start, but does not invent a session for
a failed startup. No native session identifier or error body is sent to recovery
UI. Failure remains evidence of the earlier task, even after successful recovery.

## Limits and validation

New native context receives no previous transcript. A planned retry must explicitly
prepare its agreed objective/scope/checks. Original launch environment is unchanged;
a different terminal export is not a credential refresh for a running host.
Reconnection is not account/inference verification; external startup can have its
own network behavior. Trusted scope never transfers; use restart/renewed trust.

Offline tests exercise consent/auth/strict fields, revision and idempotency,
queue-failure atomicity, fresh-profile isolation, busy task/verification/profile
exclusion, pending permission/trusted rejection, fixed errors and frozen history.
Browser acceptance covers failed initial turn, failed replacement, explicit renewed
consent and successful fresh session, unchanged host plan/draft, no task replay,
stale rejection, subsequent explicit task, and desktop/mobile layout. No hosted
Actions, live models or credentials are needed.
