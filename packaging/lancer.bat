@echo off
rem Lance la table de mixage depuis ce dossier (clé USB ou disque), sans installation.
cd /d "%~dp0"
if not exist "table-de-mixage.exe" (
  echo table-de-mixage.exe est introuvable dans %~dp0
  pause
  exit /b 1
)
start "" "%~dp0table-de-mixage.exe"
if errorlevel 1 (
  echo Windows a refuse de lancer le programme. Voir LISEZMOI.txt.
  pause
)
