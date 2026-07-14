@echo off
REM InstaSplatter gsplat CUDA trainer launcher
setlocal
cd /d "%~dp0"
python -u "%~dp0run.py"
exit /b %ERRORLEVEL%
