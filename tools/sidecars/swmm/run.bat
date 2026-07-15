@echo off
setlocal
set DIR=%~dp0
if exist "%DIR%.venv\Scripts\python.exe" (
  "%DIR%.venv\Scripts\python.exe" -u "%DIR%run.py"
) else (
  python -u "%DIR%run.py"
)
exit /b %ERRORLEVEL%
