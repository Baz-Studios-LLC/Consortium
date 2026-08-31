@echo off
REM Build and run Consortium.
REM
REM   run.cmd          build and launch in dev mode (hot reload)
REM   run.cmd build    produce an installer in src-tauri\target\release\bundle
REM
REM The CLI is staged first on purpose. tauri.conf.json runs `bundle:cli` from
REM beforeBuildCommand but has no beforeDevCommand, so a dev build otherwise
REM carries no bundled CLI and the Install CLI button fails with "bundled CLI
REM missing" — a confusing way to discover that dev and release differ.

setlocal
cd /d "%~dp0"

where cargo >nul 2>&1 || (
  echo Rust is not installed, or cargo is not on PATH. See https://rustup.rs
  exit /b 1
)
where npm >nul 2>&1 || (
  echo Node is not installed, or npm is not on PATH.
  exit /b 1
)

if not exist node_modules (
  echo Installing dependencies...
  call npm install || exit /b 1
)

echo Staging the consortium CLI...
call npm run bundle:cli || exit /b 1

if /i "%~1"=="build" (
  echo Building a release bundle...
  call npm run build
) else (
  echo Launching. First run compiles Rust from scratch and takes a few minutes.
  call npm run dev
)

endlocal
