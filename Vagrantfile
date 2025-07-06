# -*- mode: ruby -*-
# vi: set ft=ruby :

require "getoptlong"

opts = GetoptLong.new(
  ["--local", GetoptLong::NO_ARGUMENT],
  ["--headless", GetoptLong::NO_ARGUMENT],
  ["--playbook", GetoptLong::OPTIONAL_ARGUMENT],
  ["--architecture", GetoptLong::OPTIONAL_ARGUMENT],
  ["--os", GetoptLong::OPTIONAL_ARGUMENT],
  ["-f", GetoptLong::OPTIONAL_ARGUMENT],
  # Pass arguments to actual vagrant, but they're not used here
  ["--provider", GetoptLong::OPTIONAL_ARGUMENT],
  ["--help", GetoptLong::OPTIONAL_ARGUMENT],
  ["--debug", GetoptLong::OPTIONAL_ARGUMENT],
  ["--destroy-on-error", GetoptLong::OPTIONAL_ARGUMENT]
)

local = false
gui = true
os = "ubuntu/focal64"
playbook = "common-desktop.yml"
architecture = "amd64"

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
    when "--architecture"
      architecture = arg
  end
end

# Example:
# * vagrant --os=ubuntu/jammy64 --playbook=user.yml --local --headless up
# * vagrant --os=bento/ubuntu-24.04 --playbook=common-desktop.yml --local up
# * vagrant --os=cloud-image/ubuntu-24.04 --local --headless --architecture=arm64 up --provider libvirt
#
# Known problems:
# * Sometimes disk fails to set up (systemd waits on disk uuid), `vagrant destroy` and try again in this case


Vagrant.configure("2") do |config|
  config.vm.define "ansible-playbooks_#{Time.now.to_i}" do |node|
    if architecture == "arm64"
      node.vagrant.plugins = "vagrant-libvirt"
      node.vm.box_architecture = architecture
    end
    node.vm.synced_folder ".", "/vagrant", type: "rsync"

    # https://developer.hashicorp.com/vagrant/docs/vagrantfile/machine_settings
    node.vm.box = os
    node.vm.boot_timeout = 600

    # Need to run `vagrant provision` explicitly for that to work
    node.trigger.after [:provision] do |trigger|
      trigger.name = "Reboot after provisioning"
      trigger.run = { :inline => "vagrant reload" }
    end

    # https://vagrant-libvirt.github.io/vagrant-libvirt/configuration.html
    # Currently libvirt is only used for arm64. However, to destroy arm machines
    # even if the architecture  argument is not passed, we need to set all the
    # libvirt options anyway, in particular to make this fix work
    # https://github.com/vagrant-libvirt/vagrant-libvirt/pull/1329/files
    node.vm.provider "libvirt" do |libvirt|

      # Give more resources, OOMs by default
      libvirt.memory = 8000
      # Max 8 cores allowed for arm cpu
      libvirt.cpus = [`nproc`.to_i, 8].min
      libvirt.machine_type = "virt"
      # https://libvirt.org/formatdomain.html
      # In this mode, the cpu element describes the CPU that should be presented
      # to the guest. This is the default when no mode attribute is specified.
      # This mode makes it so that a persistent guest will see the same hardware
      # no matter what host the guest is booted on.
      libvirt.cpu_mode = "custom"
      libvirt.cpu_model = "cortex-a57"
      libvirt.driver = "qemu"
      libvirt.machine_arch = "aarch64"
      # Enable UEFI, refuses to work otherwise
      libvirt.nvram = "/usr/share/AAVMF/AAVMF_VARS.ms.fd"
      libvirt.loader = "/usr/share/AAVMF/AAVMF_CODE.no-secboot.fd"
      # Errors otherwise
      libvirt.inputs = []
    end

    node.vm.provider "virtualbox" do |vb|
      # Give more resources, OOMs by default
      vb.memory = 8000
      vb.cpus = `nproc`.to_i

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
      node.vm.provision "shell", env: {}, inline: <<-SHELL
        set -uex

        export DEBIAN_FRONTEND=noninteractive

        systemctl disable systemd-networkd.service

        rm -f /etc/resolv.conf
        echo "nameserver 8.8.8.8" > /etc/resolv.conf
        echo "127.0.0.1 $(cat /etc/hostname)" >> /etc/hosts

        apt-get install -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" sudo

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
        apt-get upgrade -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold"

        apt-get update
        apt-get dist-upgrade -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold"

        apt-get update
        apt-get upgrade -y

        apt-get autoremove -y
        apt purge -y '~c'

        id -u user && cd /tmp/ && sudo -u user /home/user/.bin/init-user-env.sh || true

        reboot
      SHELL
    else
      node.vm.provision "shell", env: {}, inline: <<-SHELL
        set -uex

        export DEBIAN_FRONTEND=noninteractive

        systemctl disable systemd-networkd.service

        rm -f /etc/resolv.conf
        echo "nameserver 8.8.8.8" > /etc/resolv.conf
        echo "127.0.0.1 $(cat /etc/hostname)" >> /etc/hosts

        apt-get install -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold" sudo

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
        apt-get upgrade -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold"

        apt-get update
        apt-get dist-upgrade -y -o Dpkg::Options::="--force-confdef" -o Dpkg::Options::="--force-confold"

        apt-get update
        apt-get upgrade -y

        apt-get autoremove -y
        apt purge -y '~c'

        id -u user && cd /tmp/ && sudo -u user /home/user/.bin/init-user-env.sh || true

        reboot
      SHELL
    end
  end
end
