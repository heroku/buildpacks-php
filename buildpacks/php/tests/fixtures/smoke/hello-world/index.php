<?php

require(__DIR__.'/vendor/autoload.php');

use Monolog\Logger;
use Monolog\Handler\StreamHandler;
use Bramus\Monolog\Formatter\ColoredLineFormatter;

$log = new Logger('log');
$handler = new StreamHandler('php://stderr', Logger::WARNING);
$handler->setFormatter(new ColoredLineFormatter());
$log->pushHandler($handler);

if(php_sapi_name() == "cli-server") $log->warning("You're running PHP's built-in web server, which should be used for development and testing only.");

echo \Cowsayphp\Cow::say("Hello World!");
