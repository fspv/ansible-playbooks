# -*- mode: ruby -*-
# vi: set ft=ruby :

require "getoptlong"

opts = GetoptLong.new(
  ["--local", GetoptLong::NO_ARGUMENT],
  ["--headless", GetoptLong::NO_ARGUMENT],
  ["--playbook", GetoptLong::OPTIONAL_ARGUMENT],
  ["--os", GetoptLong::OPTIONAL_ARGUMENT],
  ["-f", GetoptLong::OPTIONAL_ARGUMENT]
)

local = false
gui = true
os = "ubuntu/focal64"
playbook = "common-desktop.yml"

opts.each do |opt, arg|
  case opt
    when "--local"
      local = true
    when "--os"
      os = arg
    when "--headless"
      gui = false
    when "--playbook"
      playbook = arg
  end
end

# Example:
# * vagrant --os=ubuntu/jammy64 --playbook=user.yml --local --headless up
# * vagrant --os=bento/ubuntu-24.04 --playbook=common-desktop.yml --local up
#
# Known problems:
# * Sometimes disk fails to set up (systemd waits on disk uuid), `vagrant destroy` and try again in this case


Vagrant.configure("2") do |config|
  config.vm.box = os

  # Need to run `vagrant provision` explicitly for that to work
  config.trigger.after [:provision] do |trigger|
    trigger.name = "Reboot after provisioning"
    trigger.run = { :inline => "vagrant reload" }
  end

  config.vm.provider "virtualbox" do |vb|
    # Give more resources, OOMs by default
    vb.memory = 8000
    vb.cpus = 8

    # Make UI fast
    vb.gui = gui
    if gui
      vb.customize ["modifyvm", :id, "--vram", "128"]
      vb.customize ["modifyvm", :id, "--accelerate3d", "on"]
    end

    # Disable annoying warnings
    vb.check_guest_additions = false
  end

  if local
    config.vm.provision "shell", env: {}, inline: <<-SHELL
      set -uex

      rm -f /etc/resolv.conf
      echo "nameserver 8.8.8.8" > /etc/resolv.conf

      TMPDIR=$(mktemp -d)
      cd "${TMPDIR}"

      rm -rf ansible-playbooks
      cp -R /vagrant ansible-playbooks

      cd ansible-playbooks
      rm -rf manual
      mkdir manual

      sed 's/# //g' roles/user/defaults/main.yml > manual/common.yml

      chown -R vagrant .

      set +e
      for i in {1..3}; do
        # Run twice to make sure users are added to correct groups
        sudo -u vagrant ./bootstrap.sh #{playbook} LOCAL && \
          sudo -u vagrant ./bootstrap.sh #{playbook} LOCAL && \
          break
      done

      if [[ $? -ne 0 ]]; then
        echo "Failed to provision"
        exit 1
      fi
      set -e

      apt-get update
      apt-get upgrade -y

      apt-get update
      apt-get dist-upgrade -y

      apt-get update
      apt-get upgrade -y

      sudo apt-get autoremove -y
      sudo apt purge -y '~c'

      id -u user && cd /tmp/ && sudo -u user /home/user/.bin/init-user-env.sh || true

      reboot
    SHELL
  else
    config.vm.provision "shell", env: {}, inline: <<-SHELL
      set -uex

      rm -f /etc/resolv.conf
      echo "nameserver 8.8.8.8" > /etc/resolv.conf

      TMPDIR=$(mktemp -d)
      cd "${TMPDIR}"

      export DEBIAN_FRONTEND=noninteractive

      apt-get update

      apt-get install -y git

      rm -rf ansible-playbooks
      git clone https://github.com/fspv/ansible-playbooks.git

      cd ansible-playbooks
      mkdir manual

      sed 's/# //g' roles/user/defaults/main.yml > manual/common.yml

      chown -R vagrant .

      set +e
      if [[ $? -ne 0 ]]; then
        # Run twice to make sure users are added to correct groups
        sudo -u vagrant ./bootstrap.sh #{playbook} LOCAL && \
          sudo -u vagrant ./bootstrap.sh #{playbook} LOCAL && \
          break
      done

      if $? -ne 0; then
        echo "Failed to provision"
        exit 1
      fi
      set -e

      apt-get update
      apt-get upgrade -y

      apt-get update
      apt-get dist-upgrade -y

      apt-get update
      apt-get upgrade -y

      sudo apt-get autoremove -y
      sudo apt purge -y '~c'

      id -u user && cd /tmp/ && sudo -u user /home/user/.bin/init-user-env.sh || true

      reboot
    SHELL
  end
end
