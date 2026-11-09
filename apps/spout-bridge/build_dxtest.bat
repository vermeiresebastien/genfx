@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set SP=%~dp0..\..\_upstream\spout2\SPOUTSDK
cd /d "%~dp0"
cl /nologo /EHsc /MD /I "%SP%\SpoutDirectX\SpoutDX" /I "%SP%\SpoutGL" /I "%SP%\SpoutDirectX" ^
  dx_test.cpp ^
  "%SP%\SpoutDirectX\SpoutDX\SpoutDX.cpp" ^
  "%SP%\SpoutGL\SpoutSenderNames.cpp" "%SP%\SpoutGL\SpoutFrameCount.cpp" ^
  "%SP%\SpoutGL\SpoutCopy.cpp" "%SP%\SpoutGL\SpoutUtils.cpp" ^
  "%SP%\SpoutGL\SpoutDirectX.cpp" "%SP%\SpoutGL\SpoutSharedMemory.cpp" ^
  /Fe:dx_test.exe /link d3d11.lib dxgi.lib opengl32.lib user32.lib gdi32.lib psapi.lib shell32.lib ole32.lib
echo BUILD_EXIT=%ERRORLEVEL%
if exist dx_test.exe ( echo EXE_OK ) else ( echo EXE_MISSING )
