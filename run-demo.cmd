@echo off
chcp 65001 >nul
setlocal
set "game=D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)"
if not "%~1"=="" set "game=%~1"
if not exist "%~dp0target\release\yuka-rs.exe" (
    echo Build first with: cargo build --release --locked
    pause
    exit /b 1
)
"%~dp0target\release\yuka-rs.exe" demo "%game%" --encoding gbk --pages 100
if errorlevel 1 pause
