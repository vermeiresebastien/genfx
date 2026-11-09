@echo off
call "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat" >nul 2>&1
echo VCVARS_EXIT=%ERRORLEVEL%
where cl
cl /nologo /c /EHsc /I "%~dp0..\..\_upstream\spout2\SPOUTSDK\SpoutLibrary" "%~dp0spout_shim.cpp" /Fo:"%~dp0spout_shim.obj"
echo CL_EXIT=%ERRORLEVEL%
dir "%~dp0spout_shim.obj"
