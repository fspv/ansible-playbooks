#!/bin/bash

set -uex

sudo apt-get update

sudo apt-get -y install python-dev build-essential libssl-dev libffi-dev \
                       python-pip python-virtualenv git

. bootstrap-config.sh

rm -rf ${BOOTSTRAP_DIR}
mkdir -p ${BOOTSTRAP_DIR}

virtualenv ${ANSIBLE_VENV_DIR}

export VIRTUAL_ENV_DISABLE_PROMPT=yes

. ${ANSIBLE_VENV_DIR}/bin/activate

pip install ansible==${ANSIBLE_VERSION}

if [ "x$1" = "xLOCAL" ]
then
    cp -r . ${ANSIBLE_REPO_DIR}
elif [ "x$1" = "xREMOTE" ]
then
    git clone git@github.com:prius/ansible-playbooks.git ${ANSIBLE_REPO_DIR}
else
    echo "ERROR: You should specify either REMOTE or LOCAL as an arg to bootstrap-ansible.sh"
    exit 1
fi

if test -d manual/
then
    cp -R manual/ ${ANSIBLE_REPO_DIR}/manual/
else
    mkdir ${ANSIBLE_REPO_DIR}/manual/
    touch ${ANSIBLE_REPO_DIR}/manual/common.yml
fi
