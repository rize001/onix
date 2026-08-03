@echo off
setlocal
cd /d "%~dp0"
where cargo >nul 2>nul || (echo [Onix] Rust не установлен. Установите rustup и Rust stable.& pause & exit /b 1)
cargo build --release || (echo [Onix] Ошибка сборки.& pause & exit /b 1)
if not exist dist mkdir dist
copy /y "target\release\onix-messenger.exe" "dist\OnixMessenger.exe" >nul
echo [Onix] Готово: dist\OnixMessenger.exe
endlocal
