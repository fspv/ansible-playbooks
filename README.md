Clone
=====

```
git clone https://github.com/fspv/ansible-playbooks.git
cd ansible-playbooks
git submodule update --init --recursive
```

Examples
========

Apply local configuraiton changes

```
./bootstrap.sh common-desktop.yml LOCAL
```

Apply upstream config
```
./bootstrap.sh common-desktop.yml REMOTE
```

Dry run
```
./check.sh common-desktop.yml LOCAL
```

Playbooks
=========

# Setup Yubikey

UI: `nix-shell -p yubikey-manager-qt --run ykman-gui`
UI2: `nix-shell -p yubikey-personalization-gui --run yubikey-personalization-gui`

```bash
#!/bin/sh

mkdir -p ~/.config/Yubico
pamu2fcfg > ~/.config/Yubico/u2f_keys
echo >> ~/.config/Yubico/u2f_keys


U2F_KEYS=/etc/Yubico/u2f_keys

sudo mkdir -p /etc/Yubico
sudo touch "${U2F_KEYS}"
test -f ~/.config/Yubico/u2f_keys && cat ~/.config/Yubico/u2f_keys | sudo tee -a $U2F_KEYS

rm -rf ~/.config/Yubico

if grep -q pam_u2f.so /etc/pam.d/common-auth;
then
    true
else
    echo "auth    required            pam_u2f.so nouserok authfile=${U2F_KEYS} cue" | sudo tee -a /etc/pam.d/common-auth
fi
```

Then merge entries for a single user into one line, for example
```
<username>:<KeyHandle1>,<UserKey1>,<CoseType1>,<Options1>:<KeyHandle2>,<UserKey2>,<CoseType2>,<Options2>:<KeyHandle3>,<UserKey3>,<CoseType3>,<Options3>
```

If the key doesn't work in chromium, try replugging it.

# Setup server auth by pubkey and password

https://developers.yubico.com/SSH/Securing_SSH_with_FIDO2.html

```
PermitRootLogin no
PermitEmptyPasswords no
PasswordAuthentication yes
PubkeyAuthentication yes
AuthenticationMethods publickey,password
```

3. `systemctl restart sshd`

# Wayland tips and tricks

To enable screen capture in chromium set the following flags:

chrome://flags/#enable-webrtc-pipewire-camera -> Enabled
chrome://flags/#ozone-platform-hint -> Wayland

Prepare for the new ubuntu release
==================================

1. Create ponysay backport https://github.com/fspv/ponysay-deb
2. Test all vagrant scenarios locally
3. Test if nix builds
4. wayland apps can't be tested in vagrant, so test them locally if possible

Update grub from a Live CD
==========================
A valid strategy to avoid formatting the entire disk is to install ubuntu on a flash drive and just copy files into the old root partition.

After that you can do something like this to make it bootable again:

```sh
sudo cryptsetup luksOpen /dev/nvme0n1p3 test
sudo mount /dev/disk/by-id/dm-name-vgubuntu-root /mnt
sudo mount /dev/disk/by-id/dm-name-vgubuntu-home /mnt/home
sudo mount /dev/nvme0n1p2 /mnt/boot
sudo mount /dev/nvme0n1p1 /mnt/boot/efi
sudo mount --bind /dev /mnt/dev
sudo mount --bind /sys /mnt/sys
sudo mount --bind /proc /mnt/proc
sudo chroot /mnt
```

Then you can copy old `/etc/fstab` and `/etc/crypttab` to the new system. When you format partition, its uuid changes, so you need to update `/etc/crypttab` and `/etc/fstab`.

Finally, you can update grub with
```sh
update-initramfs -k all -u
update-grub
grub-install /dev/nvme0n1
```

If everything is done correctly, the system should boot again.

Debug initramfs
===============

https://wiki.debian.org/InitramfsDebug

1. Try add `break` to the kernel command line
2. Inspect initrd image contents with `lsinitramfs /boot/initrd.img`
3. You can unpack initramfs image with `unmkinitramfs /boot/initrd.img /tmp/initrd`
4. Try to explore differences between working and non-working initramfs images with `diffoscope --exclude-directory-metadata=yes /tmp/initrd1 /tmp/initrd2` (can be installed with `nix-shell -p diffoscope`)

Recent initramfs bugs can be found here https://bugs.launchpad.net/ubuntu/+source/initramfs-tools/+bugs?orderby=-id&start=0
