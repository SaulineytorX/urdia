#!/usr/bin/env python3
"""Install optional user-local browser test tooling; never sudo or replace a runtime."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

def main():
    if sys.platform!='linux':raise ValueError('Run inside Linux/WSL.')
    root=Path.home()/'.local/share/urdia/frontend-checks'
    if root.exists() or root.is_symlink():raise ValueError('Existing runtime preserved; review it before reinstalling.')
    if input('Install optional Playwright 1.58.0 in your user directory? Downloads packages. [y/N] ').strip().lower() not in ('y','yes'):return 0
    for path in (root.parent,*root.parent.parents):
        if path.is_symlink():raise ValueError('Redirected installation path refused.')
    env={k:v for k,v in os.environ.items() if k in ('PATH','HOME','LANG','LC_ALL','TMPDIR')}
    root.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryFile() as log:
        subprocess.run([sys.executable,'-m','venv',str(root)],env=env,stdout=log,stderr=log,check=True)
        root.chmod(0o700)
        subprocess.run([str(root/'bin/python'),'-m','pip','--isolated','install','--index-url','https://pypi.org/simple','playwright==1.58.0'],env=env,stdout=log,stderr=log,check=True)
    print('Optional runtime installed. A separately installed Linux Chrome with a working sandbox is required. No browser or system packages installed.')
    return 0
if __name__=='__main__':
    try:raise SystemExit(main())
    except (ValueError,OSError,EOFError,subprocess.SubprocessError):
        print('Frontend runtime setup refused or failed; existing installations were not replaced. Review docs/frontend-checks.md.',file=sys.stderr);raise SystemExit(1)
