#!/bin/bash -uex

if VBoxManage list vms | grep $1
then
    echo "Killing VM $1"
else
    exit
fi

if VBoxManage list runningvms | grep $1
then
    VBoxManage controlvm $1 poweroff
fi

while VBoxManage list runningvms | grep $1
do
    sleep 10
    echo "Waiting for vm $1 to be powered off"
done

VBoxManage unregistervm $1 --delete
