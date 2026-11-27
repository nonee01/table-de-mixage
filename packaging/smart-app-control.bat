@echo off
rem Smart App Control bloque tout programme non signe : la table de mixage, et pendant
rem construire.bat les scripts de construction de Rust. S'il est actif, on propose de le desactiver.
rem Sortie 0 : il est desactive (ou l'etait deja).  "/ci" : sans question.
setlocal
set "CLE=HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy"
reg query "%CLE%" /v VerifiedAndReputablePolicyState 2>nul | find "0x1" >nul
if errorlevel 1 exit /b 0
echo.
echo  Smart App Control est ACTIVE sur ce PC : il bloque tout programme non signe,
echo  dont la table de mixage. Il faut le desactiver.
echo  Pour le reactiver apres la seance : Securite Windows ^>
echo  Controle des applications et du navigateur ^> Smart App Control.
echo.
if /i "%~1"=="/ci" goto main
choice /c ON /m " Le desactiver maintenant ? Windows demandera l'autorisation administrateur"
if errorlevel 2 goto main
rem 0 = desactive, puis CiTool l'applique sans redemarrer (-json : sans attendre Entree)
powershell -NoProfile -Command "try { $p = Start-Process cmd -ArgumentList '/c reg add %CLE% /v VerifiedAndReputablePolicyState /t REG_DWORD /d 0 /f && CiTool.exe -r -json' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode } catch { exit 1 }"
set "CODE=%errorlevel%"
reg query "%CLE%" /v VerifiedAndReputablePolicyState 2>nul | find "0x1" >nul
if not errorlevel 1 goto main
if not "%CODE%"=="0" goto redemarrer
echo  Smart App Control : desactive.
echo.
exit /b 0
:redemarrer
echo  Smart App Control est desactive, mais Windows ne l'applique qu'apres un redemarrage.
echo  Redemarrez le PC puis relancez.
exit /b 1
:main
echo  A faire a la main : Parametres ^> Confidentialite et securite ^> Securite Windows ^>
echo  Controle des applications et du navigateur ^> Smart App Control ^> Desactive.
echo  Puis relancez.
exit /b 1
