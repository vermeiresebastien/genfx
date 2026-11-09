@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set SDK=%~dp0..\..\_upstream\spout2\SPOUTSDK\SpoutLibrary
set SRC=%~dp0spout_shim.cpp
set OUT=%~dp0
cl /nologo /LD /EHsc /MD /I "%SDK%" "%SRC%" /Fe:"%OUT%spout_shim.dll" /link /LIBPATH:"%SDK%\Binaries" SpoutLibrary.lib opengl32.lib user32.lib gdi32.lib 2>&1
echo LINK_EXIT=%ERRORLEVEL%
if exist "%OUT%spout_shim.dll" (
  copy /Y "%SDK%\Binaries\SpoutLibrary.dll" "%OUT%" >nul
  echo DLL_OK
) else (
  echo DLL_MISSING
)
