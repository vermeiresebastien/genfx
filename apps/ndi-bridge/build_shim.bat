@echo off
REM Build the NDI shim (ndi_shim.dll) against the official NDI headers.
REM Generates an import lib from the installed NDI runtime DLL, then compiles+links.
setlocal
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1

set INC=C:\Users\Sebastien\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\ndi-sdk-0.2.0\vendored\include
set RTDLL=C:\Program Files\NDI\NDI 6 Tools\Runtime\Processing.NDI.Lib.x64.dll
set OUT=%~dp0

if not exist "%INC%\Processing.NDI.Lib.h" (
  echo MISSING_HEADERS: %INC%
  exit /b 1
)
if not exist "%RTDLL%" (
  echo MISSING_RUNTIME: %RTDLL%
  exit /b 1
)

REM 1) Generate import lib from the runtime DLL exports.
dumpbin /nologo /exports "%RTDLL%" > "%OUT%exports.txt" 2>&1
echo LIBRARY Processing.NDI.Lib.x64.dll > "%OUT%ndi.def"
echo EXPORTS >> "%OUT%ndi.def"
findstr /R "^ *[0-9]* *[0-9A-F]* *NDIlib_" "%OUT%exports.txt" | for /f "tokens=4" %%A in ('findstr /R "^ *[0-9]* *[0-9A-F]* *NDIlib_" "%OUT%exports.txt"') do echo %%A >> "%OUT%ndi.def"
lib /nologo /def:"%OUT%ndi.def" /machine:x64 /out:"%OUT%Processing.NDI.Lib.x64.lib" >nul 2>&1
echo IMPORTLIB_EXIT=%ERRORLEVEL%

REM 2) Compile + link the shim.
cl /nologo /LD /EHsc /MD /I "%INC%" "%OUT%ndi_shim.cpp" /Fe:"%OUT%ndi_shim.dll" /link /LIBPATH:"%OUT%" Processing.NDI.Lib.x64.lib 2>&1
echo LINK_EXIT=%ERRORLEVEL%

REM 3) Copy the runtime next to the shim for LOAD-time resolution.
copy /Y "%RTDLL%" "%OUT%" >nul
if exist "%OUT%ndi_shim.dll" ( echo DLL_OK ) else ( echo DLL_MISSING )
