@rem 以 UTF-8 编译 Windows 资源，支持中文产品名称。
@echo off
"%CHATVAULT_RESOURCE_COMPILER%" /C 65001 %*
exit /b %errorlevel%
