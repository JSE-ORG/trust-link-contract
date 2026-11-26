@echo off
REM TrustLink Mainnet Deployment Script for Windows
REM Delegates to deploy.sh via bash/WSL if available, otherwise provides guidance

setlocal enabledelayedexpansion

if "%~1"=="--help" (
    echo TrustLink Mainnet Deployment Wrapper
    echo.
    echo Usage: deploy.bat [--network testnet^|public] [options]
    echo.
    echo This script requires the Stellar CLI installed.
    echo On Windows, it's recommended to use WSL2 or Git Bash.
    echo.
    echo For full documentation, see: docs/runbook-release.md
    echo.
    exit /b 0
)

REM Try to find bash (WSL, Git Bash, or native bash)
where bash >nul 2>&1
if !errorlevel! equ 0 (
    bash scripts\deploy.sh %*
    exit /b !errorlevel!
)

where wsl >nul 2>&1
if !errorlevel! equ 0 (
    wsl bash scripts/deploy.sh %*
    exit /b !errorlevel!
)

echo.
echo ^^! Bash not found. Installation required:
echo.
echo Option 1 - WSL2 (Recommended^):
echo   1. wsl --install
echo   2. wsl --set-default-version 2
echo   3. Install Ubuntu from Microsoft Store
echo   4. wsl bash scripts/deploy.sh --network testnet
echo.
echo Option 2 - Git Bash:
echo   1. Install from https://git-scm.com/download/win
echo   2. "Git Bash Here" on project folder
echo   3. bash scripts/deploy.sh --network testnet
echo.
echo Option 3 - Manual deployment (see docs/runbook-release.md^):
echo   1. Install Stellar CLI: https://github.com/stellar/stellar-cli
echo   2. stellar contract deploy --network testnet ^^
echo      --source-account mainnet-deployer ^^
echo      --wasm target/wasm32v1-none/release/trustlink_escrow.wasm
echo.
exit /b 1
