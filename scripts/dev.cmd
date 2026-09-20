@echo off
rem Launch HL Rating in development mode: builds the Rust side, starts Vite,
rem and opens the app window. Close the window (or this one) to stop it.
rem
rem Double-click a copy of this on the Desktop, or run `scripts\dev.cmd`.

title HL Rating (dev)

rem This file lives in <repo>\scripts, so the repo is one level up.
cd /d "%~dp0.."

rem cargo is not always on the PATH of a fresh shell.
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"

where cargo >nul 2>&1
if errorlevel 1 (
  echo Rust is not installed, or cargo is not on the PATH.
  echo Install it from https://rustup.rs and run this again.
  pause
  exit /b 1
)

if not exist "node_modules" (
  echo First run: installing dependencies, which takes a minute...
  call npm install || goto :failed
  call npm install --prefix ui || goto :failed
)

echo.
echo Starting HL Rating. The first build can take a few minutes; after that it
echo reloads itself when the code changes.
echo.
call npm run dev
if errorlevel 1 goto :failed
exit /b 0

:failed
echo.
echo It stopped with an error. The lines above say why.
pause
exit /b 1
