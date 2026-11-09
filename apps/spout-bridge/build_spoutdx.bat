@echo off
REM Build the Spout2 DX11 sender bridge (spoutdx_shim.dll).
REM Creates a D3D11 device via SpoutDX and sends RGBA8 by uploading to a D3D11
REM texture -> SendTexture (no OpenGL context needed).
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
set SPOUT=%~dp0..\..\_upstream\spout2\SPOUTSDK
set OUT=%~dp0

REM Compile the shim + SpoutDX.cpp into one DLL. SpoutDX.cpp pulls its own
REM dependencies via headers; SpoutGL sources needed by SpoutDX are compiled too.
cl /nologo /LD /EHsc /MT /D_USRDLL /I "%SPOUT%\SpoutDirectX\SpoutDX" /I "%SPOUT%\SpoutGL" /I "%SPOUT%\SpoutDirectX" ^
   "%OUT%spoutdx_shim.cpp" ^
   "%SPOUT%\SpoutDirectX\SpoutDX\SpoutDX.cpp" ^
   "%SPOUT%\SpoutGL\SpoutSenderNames.cpp" ^
   "%SPOUT%\SpoutGL\SpoutFrameCount.cpp" ^
   "%SPOUT%\SpoutGL\SpoutCopy.cpp" ^
   "%SPOUT%\SpoutGL\SpoutUtils.cpp" ^
   "%SPOUT%\SpoutGL\SpoutDirectX.cpp" ^
   "%SPOUT%\SpoutGL\SpoutSharedMemory.cpp" ^
   /Fe:"%OUT%spoutdx_shim.dll" /link d3d11.lib dxgi.lib opengl32.lib user32.lib gdi32.lib psapi.lib shell32.lib ole32.lib 2>&1
echo LINK_EXIT=%ERRORLEVEL%
if exist "%OUT%spoutdx_shim.dll" ( echo DX_DLL_OK ) else ( echo DX_DLL_MISSING )
