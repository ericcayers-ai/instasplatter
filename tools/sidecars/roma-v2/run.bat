@echo off
REM RoMa v2 densify launcher (Windows). Prefer a venv python when present.
setlocal
set DIR=%~dp0
if exist "%DIR%.venv\Scripts\python.exe" (
  "%DIR%.venv\Scripts\python.exe" "%DIR%run.py"
) else (
  python "%DIR%run.py"
)
