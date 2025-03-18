#!/bin/bash

set -uex


if which apt-get 2>&1 >/dev/null
then
    sudo apt-get update

    sudo apt-get -y install build-essential libssl-dev libffi-dev \
			       python3-pip python3-virtualenv git \
			       python3-dev pkg-config acl
fi

if which yum 2>&1 >/dev/null
then
    sudo yum -y install python-pip python-virtualenv git
fi

. bootstrap-config.sh

rm -rf ${BOOTSTRAP_DIR}
mkdir -p ${BOOTSTRAP_DIR}

unset _PYTHON_SYSCONFIGDATA_NAME

DEB_PYTHON_INSTALL_LAYOUT='deb' virtualenv -p python3 ${ANSIBLE_VENV_DIR}

export VIRTUAL_ENV_DISABLE_PROMPT=yes

. ${ANSIBLE_VENV_DIR}/bin/activate

pip install ansible==${ANSIBLE_VERSION}

if [ "x$1" = "xLOCAL" ]
then
    cp -r . ${ANSIBLE_REPO_DIR}
elif [ "x$1" = "xREMOTE" ]
then
    git clone --recurse-submodules https://github.com/fspv/ansible-playbooks.git ${ANSIBLE_REPO_DIR}
else
    echo "ERROR: You should specify either REMOTE or LOCAL as an arg to bootstrap-ansible.sh"
    exit 1
fi

mkdir -p ~/.ansible
rm -f ~/.ansible/plugins
ln -sfnT ${ANSIBLE_REPO_DIR}/plugins ~/.ansible/plugins

if test -d manual/
then
    cp -R manual/ ${ANSIBLE_REPO_DIR}/manual/
else
    mkdir ${ANSIBLE_REPO_DIR}/manual/
    touch ${ANSIBLE_REPO_DIR}/manual/common.yml
fi
