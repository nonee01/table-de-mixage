@echo off
rem Installe ce qui manque (outils C++ de Microsoft, Rust), construit la table de mixage
rem depuis les sources de ce dossier, ajoute la voix de synthese, puis la lance.
rem Relancer ce fichier ne refait que ce qui manque.  "construire.bat /ci" : sans lancer ni pause.
setlocal
cd /d "%~dp0"
set "CURL=%SystemRoot%\System32\curl.exe"
set "TAR=%SystemRoot%\System32\tar.exe"
set "SORTIE=%~dp0dist\Table de mixage"
set "TEMPO=%TEMP%\table-de-mixage"
rem les ~2 Go de construction restent sur le disque du PC, pas sur la cle USB
set "CARGO_TARGET_DIR=%LOCALAPPDATA%\table-de-mixage\target"
if not exist "%TEMPO%" mkdir "%TEMPO%"

echo.
echo  TABLE DE MIXAGE - installation et construction
echo  La premiere fois : environ 5 Go a telecharger et 30 a 45 minutes. Internet necessaire.
echo.

call "packaging\smart-app-control.bat" %1
if errorlevel 1 goto fin
if not exist "%CURL%" goto vieux_windows
if not exist "%TAR%" goto vieux_windows

rem ---- 1. outils C++ de Microsoft (l'editeur de liens dont Rust a besoin)
set "VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
set "VC="
del "%TEMPO%\vc.txt" 2>nul
if exist "%VSWHERE%" "%VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath > "%TEMPO%\vc.txt"
if exist "%TEMPO%\vc.txt" set /p VC=<"%TEMPO%\vc.txt"
if defined VC goto rust
echo [1/4] Outils C++ de Microsoft : environ 3 Go, 15 a 30 minutes.
echo       Windows demande l'autorisation administrateur : repondez Oui.
"%CURL%" -fSL -o "%TEMPO%\vs_BuildTools.exe" https://aka.ms/vs/17/release/vs_BuildTools.exe || goto echec
powershell -NoProfile -Command "$p = Start-Process -FilePath (Join-Path $env:TEMPO 'vs_BuildTools.exe') -ArgumentList '--quiet','--wait','--norestart','--nocache','--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended' -Verb RunAs -Wait -PassThru; exit $p.ExitCode"
rem 3010 = installe, redemarrage conseille plus tard
if errorlevel 3010 if not errorlevel 3011 goto rust
if errorlevel 1 goto echec

:rust
echo [1/4] Outils C++ : OK
rem ---- 2. Rust
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where cargo >nul 2>nul && goto construire
echo [2/4] Rust : environ 300 Mo.
"%CURL%" -fSL -o "%TEMPO%\rustup-init.exe" https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe || goto echec
"%TEMPO%\rustup-init.exe" -y --profile minimal --default-toolchain stable || goto echec

:construire
echo [2/4] Rust : OK
rem ---- 3. le programme
echo [3/4] Construction : 5 a 10 minutes la premiere fois, quelques secondes ensuite.
cargo build --release -p table-de-mixage || goto echec

rem ---- 4. le dossier final, avec Piper et la voix francaise
echo [4/4] Dossier final et voix de synthese
if not exist "%SORTIE%\voix" mkdir "%SORTIE%\voix"
copy /y "%CARGO_TARGET_DIR%\release\table-de-mixage.exe" "%SORTIE%\" >nul || goto echec
if exist "%SORTIE%\piper\piper.exe" goto voix
"%CURL%" -fSL -o "%TEMPO%\piper.zip" https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip || goto echec
"%TAR%" -xf "%TEMPO%\piper.zip" -C "%SORTIE%" || goto echec
:voix
set "VOIX=%SORTIE%\voix\fr_FR-siwis-medium.onnx"
if exist "%VOIX%" goto fichiers
rem telechargee a cote puis renommee : un telechargement coupe ne laisse pas une voix abimee
"%CURL%" -fSL -o "%TEMPO%\voix.onnx" https://huggingface.co/rhasspy/piper-voices/resolve/main/fr/fr_FR/siwis/medium/fr_FR-siwis-medium.onnx || goto echec
move /y "%TEMPO%\voix.onnx" "%VOIX%" >nul || goto echec
:fichiers
copy /y "packaging\voix\fr_FR-siwis-medium.onnx.json" "%SORTIE%\voix\" >nul || goto echec
copy /y "packaging\lancer.bat" "%SORTIE%\Lancer la table de mixage.bat" >nul || goto echec
copy /y "packaging\smart-app-control.bat" "%SORTIE%\" >nul || goto echec
copy /y "packaging\LISEZMOI.txt" "%SORTIE%\" >nul || goto echec

echo.
echo  Termine. La prochaine fois, double-cliquez simplement sur
echo  dist\Table de mixage\Lancer la table de mixage.bat
echo.
if /i "%~1"=="/ci" exit /b 0
call "%SORTIE%\Lancer la table de mixage.bat"
exit /b 0

:vieux_windows
echo  curl ou tar manque : il faut Windows 10 version 1803 ou plus recent.
goto fin

:echec
echo.
echo  ECHEC de l'etape ci-dessus. Verifiez la connexion Internet et relancez ce fichier :
echo  ce qui est deja installe ne sera pas refait.

:fin
if /i "%~1"=="/ci" exit /b 1
pause
exit /b 1
