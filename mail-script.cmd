@ECHO OFF
SET START_TIME=%date% %time%
PUSHD "%~dp0"

:: Configurations
SET SERVER=localhost
SET PORT=25
SET AUTH=noauth
::SET USERNAME=username
::SET PASSWORD=password

:: Prep Log
SET LOG_FILE=logs\%date:~7,2%-%date:~4,2%-%date:~-4%_%time:~0,2%%time:~3,2%%time:~6,2%.log
MKDIR logs 2> NUL

:: Run & Log to file
ECHO. >> %LOG_FILE%
ECHO %START_TIME% >> %LOG_FILE%
ECHO =================================== >> %LOG_FILE%
mail-script.exe >> %LOG_FILE% 2>&1

SET EXIT_CODE=%ERRORLEVEL%

:: Read the log file for the caller
TYPE %LOG_FILE%

EXIT /B %EXIT_CODE%
