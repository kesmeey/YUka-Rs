@echo off
chcp 65001 >nul
setlocal
set "game=D:\gal\love loabe\【PC】情侣热恋中\情侣热恋中（ラブラブル)"
if not "%~1"=="" set "game=%~1"
set "player=%~dp0bin\yuka-rs.exe"
if not exist "%player%" set "player=%~dp0target\release\yuka-rs.exe"
if not exist "%player%" (
    echo Build first with: cargo build --release --locked
    pause
    exit /b 1
)
"%player%" play "%game%" --encoding gbk --save-dir "%~dp0saves"
if errorlevel 1 pause
