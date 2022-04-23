# -*- mode: ruby -*-
# vi: set ft=ruby :

Vagrant.configure("2") do |config|
  config.vm.box = "ubuntu/focal64"

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
    vb.gui = true
    vb.customize ["modifyvm", :id, "--vram", "128"]
    vb.customize ["modifyvm", :id, "--accelerate3d", "on"]

    # Disable annoying warnings
    vb.check_guest_additions = false
  end

  config.vm.provision "shell", env: {}, inline: <<-SHELL
    set -x

    export DEBIAN_FRONTEND=noninteractive

    apt-get update

    apt-get install -y git

    rm -rf ansible-playbooks
    git clone https://github.com/prius/ansible-playbooks.git

    cd ansible-playbooks
    mkdir manual

    sed 's/# //g' roles/user/defaults/main.yml > manual/common.yml

    ./bootstrap.sh common-desktop.yml REMOTE
  SHELL
end
