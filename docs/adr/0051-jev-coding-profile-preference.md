# ADR 0051: Separate coding-profile preference from generic routing

Status: accepted for private experimental preview, 2026-10-09.

The first live calculator task among three configured profiles escalated instead
of selecting one. Generic routing requires confirmed evidence/capability fit;
coding-profile preference also carried missing benchmark/authentication fields.
Our hypothesis was that this mixed two judgments: a plausible model/effort choice
and a guarantee of verified capability, quality or authorization.

Add an explicit host-selected Jev profile-preference question. It compares declared
coding profiles against task requirements and complexity, permits a reasonable
preference without benchmark guarantees, and favors suitable lower declared
effort when alternatives appear adequate. It neither claims measured savings nor
grants tools. Missing essential coverage, ambiguity, unsafe scope, credential
extraction or permission bypass still have a no-match outcome.

The adapter's default question remains byte-for-byte the existing generic routing
instructions/criteria. Input schema, state and objective cannot select the new
mode: only the host builder can. Browser profile advice selects it when its
host-prepared profile map is nonempty; connection probes and CLI generic routing
keep the original mode. All typed-response validation, fallback, exact selectors,
startup and permission checks are unchanged.

The revised calculator pilot chose Codex low and passed independent checks. This
is development evidence after observing the initial failure, not a blind test,
production-quality pass or proof of optimal/cheapest selection. Earlier generic
and profile evaluation results must not be transferred to the new question.
ADR0015 and ADR0050's experimental limits remain applicable. A new preregistered
profile evaluation is needed before claiming selection quality.
