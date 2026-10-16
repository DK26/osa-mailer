@ECHO OFF
SET START_TIME=%date% %time%
PUSHD "%~dp0"

:: Configurations
SET SERVER=localhost
SET PORT=25
SET AUTH=noauth
::SET USERNAME=username
::SET PASSWORD=password
:: Per-e-mail SMTP timeout in seconds (default 60)
::SET SMTP_TIMEOUT_SECS=60
:: Log level: error | warn | info | debug (default info)
::SET OSA_LOG=info

:: Run. Logging is done by the application itself: stdout plus
:: logs\osa_mailer.log (locale-free name, see src/logging.rs). The previous
:: %date%-based log file redirection is deprecated — it broke on
:: non-English system locales (G4-3) and captured raw, unfiltered output.
:: The exit code is passed through unchanged so the scheduler sees
:: failures (G1-13).
osa_mailer.exe

EXIT /B %ERRORLEVEL%
