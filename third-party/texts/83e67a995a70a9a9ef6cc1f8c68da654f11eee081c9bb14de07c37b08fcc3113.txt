---
license: apache-2.0
library_name: transformers
pipeline_tag: text-classification
language: [en]
tags: [laya, system-one, calibrated-decisions, rlcd, typed-decisions, classification, agent-observability, invoice-processing, security-incidents, customer-service, commercial-use]
---

<p align="center">
  <img src="https://huggingface.co/convaiinnovations/laya/resolve/main/assets/logo-mark.png" alt="" width="72" />
</p>

# Laya Typed-Decisions

Non-autoregressive **System 1 decision model**, fine-tuned on the
[typed-decisions](https://huggingface.co/datasets/LocalLLaMA/typed-decisions) workflows: agent-trace
observability, customer service, invoice processing and security incidents.

Part of the [Laya family](https://huggingface.co/convaiinnovations/laya).

| checkpoint | encoder | params | context | use it for |
|---|---|---|---|---|
| [`convaiinnovations/laya`](https://huggingface.co/convaiinnovations/laya) | ModernBERT-large | 421M | 512 | English, general |
| [`convaiinnovations/laya-multilingual`](https://huggingface.co/convaiinnovations/laya-multilingual) | mmBERT-base | 322M | 1024 | 100+ languages |
| **`convaiinnovations/laya-typed-decisions`** (this repo) | ModernBERT-large | 421M | 1024 | these four workflows |

## Benchmark

400 test cases, 2,000 decisions, measured on the official test split.

| model | accuracy | soft acc | Brier | ECE | score MAE |
|---|---|---|---|---|---|
| **this checkpoint** | **0.766** | 0.471 | **0.062** | 0.213 | **0.242** |
| *TypeSafe Jev 1.13.0 (published)* | *0.727* | *0.580* | *0.148* | *0.144* | *0.391* |
| *teacher self-agreement ceiling* | *0.735* | | | | |
| *ModernBERT-base specialist (published)* | *0.646* | | | | |
| *per-question majority class* | *0.461* | | | | |
| *random guess* | *0.318* | | | | |
| `laya` (not fine-tuned) | 0.362 | 0.332 | 0.316 | 0.175 | 0.694 |
| `laya-multilingual` (not fine-tuned) | 0.342 | 0.326 | 0.439 | 0.285 | 0.687 |

**+3.9 points over Jev's published 0.727, above the 0.735 teacher ceiling**, with 2.4x better
Brier and 1.6x better score MAE.

Jev figures are third-party published, not measured here — there is no TypeSafe API access in
this project, and sample sizes and prompts differ. Treat the comparison as indicative.

### By workflow

| workflow | accuracy |
|---|---|
| invoice processing | **0.804** |
| security incidents | 0.766 |
| customer service | 0.764 |
| agent-trace observability | 0.730 |

### By primitive

| type | accuracy | ECE | n |
|---|---|---|---|
| `noul` | **0.857** | 0.192 | 600 |
| `choice` | 0.733 | 0.255 | 600 |
| `score` | 0.723 | 0.199 | 800 |

## Quickstart

```bash
pip install laya
```

```python
import laya

agent = laya.load("convaiinnovations/laya-typed-decisions")
result = agent.predict(state, questions)
```

Or route to it explicitly:

```python
from laya import Router

router = Router()
router.predict(state, questions, model="typed-decisions")
```

`Router` will not select this checkpoint automatically unless you construct it with
`auto_task_detection=True` — it is specialised to four synthetic workflows and should not be a
silent default.

If this checkpoint is on a hot path, keep it resident rather than loading it per request:

```python
router = Router()
router.preload(["typed-decisions"])          # or router.attach("typed-decisions", agent)
```

`preload` fetches and builds the checkpoints you name once; `attach` registers an `Agent` you
already hold, so nothing is loaded twice.

`laya.load()` builds the model without a throwaway random initialisation, so
loading is about 10x faster with identical answers.

`agent.predict_batch(states, questions)` scores many states in shared forward
passes, with answers identical to one call per state, which suits workflow triage over a queue.

> **If `laya.load()` hangs:** `transformers` probes for TensorFlow at import, and when TF is
> installed its abseil runtime can deadlock model construction. Run with `USE_TF=0`.

## Training

Fine-tuned from [`convaiinnovations/laya`](https://huggingface.co/convaiinnovations/laya) on the
benchmark's 1,200-case training split (6,000 decisions) with **RLCD**: the policy reports a
distribution, exploration adds zero-mean Gaussian noise to the logits, and the reward is a
strictly proper scoring rule (log + spherical, plus ranked probability score for ordinal
questions), so expected reward is maximised only by honest probabilities. Updates are REINFORCE
with a group-mean baseline, alongside soft cross-entropy against the teacher's distributions.

Reproduce it:
[`laya_finetune_typed_decisions_2xT4_kaggle.ipynb`](https://github.com/NandhaKishorM/laya/blob/main/notebooks/laya_finetune_typed_decisions_2xT4_kaggle.ipynb)
— about 4–5 hours on Kaggle's free 2xT4.

## Limits

- **This is a specialist.** It was fine-tuned on four specific synthetic workflows. Expect it to
  behave like the base `laya` checkpoint, or worse, on anything else.
- **Soft accuracy trails Jev** (0.471 vs 0.580): its argmax is better, but its probability
  *distributions* match the teacher less well.
- **Still over-confident** (ECE 0.213 vs Jev's 0.144). Its `temperature_by_options` was inherited
  from the base checkpoint and overrides the per-type temperatures fitted for this model — refit
  on your own held-out data before relying on the probabilities.
- **Its per-type temperatures were fitted on training data.** The fine-tuning run fitted
  `[1.0148, 1.0374, 1.0575]` on a slice of the same items it had just trained on, which is why they
  sit so close to 1.0 ([#186](https://github.com/NandhaKishorM/laya/issues/186)). The notebook now
  holds that slice out of training. Until this checkpoint is refit, treat its confidence as
  uncalibrated.
- English only. Use `laya-multilingual` for other languages.
- **Keep `choice` questions under ~20 options.** Options share a fixed 256-token head budget, so
  a large label space leaves few tokens per label and accuracy falls off sharply.

## Links

- **Docs** https://nandhakishorm.github.io/laya/
- **Hub / family** https://huggingface.co/convaiinnovations/laya
- **GitHub** https://github.com/NandhaKishorM/laya · full benchmark data on the `research` branch
- **PyPI** https://pypi.org/project/laya/
- **Benchmark dataset** https://huggingface.co/datasets/LocalLLaMA/typed-decisions

Apache 2.0 · Convai Innovations
