"""Restricted Laya server entry point for Urdia.

The upstream server supports several checkpoints and network bindings. Urdia
loads one reviewed checkpoint from a local, revision-pinned snapshot and binds
the HTTP service to a fixed loopback socket.
"""

from __future__ import annotations

import os
from pathlib import Path

import laya
import uvicorn
from laya import Router
from laya.serve import create_app

LAYA_VERSION = "0.3.20"
MODEL_NAME = "typed-decisions"
MODEL_PATH = Path("/opt/urdia/laya/model")
HOST = "127.0.0.1"
PORT = 18765


def main() -> None:
    if laya.__version__ != LAYA_VERSION:
        raise RuntimeError("the installed Laya version does not match Urdia's pin")
    if not MODEL_PATH.is_dir():
        raise RuntimeError("the pinned local Laya model is missing")
    if not os.environ.get("LAYA_API_KEY"):
        raise RuntimeError("Laya bearer authentication is required")

    router = Router(
        models={MODEL_NAME: str(MODEL_PATH)},
        device="cpu",
        max_loaded=1,
        auto_task_detection=False,
    )
    # Refuse every checkpoint except the one Urdia has pinned locally. This also
    # prevents an authenticated client from triggering an unreviewed download.
    router.models = {MODEL_NAME: str(MODEL_PATH)}
    router.preload([MODEL_NAME])

    uvicorn.run(
        create_app(router),
        host=HOST,
        port=PORT,
        log_level="info",
        access_log=False,
    )


if __name__ == "__main__":
    main()
