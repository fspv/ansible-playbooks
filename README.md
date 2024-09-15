Clone
=====

```
git clone https://github.com/prius/ansible-playbooks.git
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

# Setup Yubikey for server

0. Preserve a stable root ssh session to prevent locking you out

1. Validate OTP https://demo.yubico.com/otp/verify

2. /etc/ssh/sshd_config

```
PermitRootLogin yes
KbdInteractiveAuthentication yes
UsePAM yes
AuthenticationMethods publickey,keyboard-interactive:pam
```

2. /etc/pam.d/sshd

```
auth sufficient pam_yubico.so id=1234 debug authfile=/etc/yubikey
```

3. `systemctl restart sshd`

# Wayland tips and tricks

To enable screen capture in chromium set the following flags:

chrome://flags/#ozone-platform-hint -> Wayland

chrome://flags/#enable-webrtc-pipewire-capturer -> Enable

And run this
```
systemctl --user enable pipewire-media-session
systemctl --user start pipewire-media-session
systemctl --user enable wireplumber.service
systemctl --user start wireplumber.service
systemctl --user restart xdg-desktop-portal-gnome
systemctl --user restart xdg-desktop-portal.service
systemctl --user enable xdg-desktop-portal-wlr.service
systemctl --user start xdg-desktop-portal-wlr.service
```

If can't connect bluetooth, run
```
systemctl --user restart wireplumber.service
```

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

Then you can copy old /etc/fsab and /etc/crypttab to the new system. When you format partition, its uuid changes, so you need to update /etc/crypttab and /etc/fstab.

Finally, you can update grub with
```sh
sudo grub-install /dev/nvme0n1
sudo update-grub
```

If everything is done correctly, the system should boot again.
