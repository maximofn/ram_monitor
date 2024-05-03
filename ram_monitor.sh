#!/usr/bin/bash
# Get script path
SCRIPT_PATH=$(dirname $0)
/usr/bin/python3 $SCRIPT_PATH/ram_monitor.py >ram_monitor.log 2>ram_monitor_error.log