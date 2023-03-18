#!/bin/bash

set -uex

if [ "$#" -eq 2 ]
then
    ./bootstrap-ansible.sh $2
else
    ./bootstrap-ansible.sh REMOTE
fi

ANSIBLE_ROLE=$1
ANSIBLE_ARGS=""

. bootstrap-config.sh

if test -f ${ANSIBLE_REPO_DIR}/ansible-default
then
    . ${ANSIBLE_REPO_DIR}/ansible-default
fi

if test -f ${ANSIBLE_REPO_DIR}/manual/ansible-default
then
    . ${ANSIBLE_REPO_DIR}/manual/ansible-default
fi

ANSIBLE_ARGS="${ANSIBLE_ARGS} --check $1"

cd ${ANSIBLE_REPO_DIR}

set +u
. ${ANSIBLE_VENV_DIR}/bin/activate
set -u

ansible-playbook ${ANSIBLE_ARGS}
