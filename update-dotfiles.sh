#!/bin/sh -uex

cd roles/user/files/home

git pull origin master

cd -

if [ -z "$(git status --porcelain roles/user/files/home)" ]; then
  echo "No changes in roles/user/files/home"
  exit 0
fi

git add roles/user/files/home
git commit -m "update dotfiles"
