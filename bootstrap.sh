#!/bin/bash

set -uex

apt-get update

apt-get -y install python-dev build-essential libssl-dev libffi-dev \
                       python-pip python-virtualenv git

mkdir -p /opt/venvs/

cd /opt/venvs/

if ! test -d /opt/venvs/ansible/
then
    virtualenv ansible

    export VIRTUAL_ENV_DISABLE_PROMPT=yes

    . /opt/venvs/ansible/bin/activate && pip install ansible==2.4.0.0
fi
