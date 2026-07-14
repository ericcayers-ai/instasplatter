@echo off
set DIR=%~dp0
if exist "%DIR%.venv\Scripts\python.exe" (
  "%DIR%.venv\Scripts\python.exe" "%DIR%run.py"
) else (
  python "%DIR%run.py"
)
