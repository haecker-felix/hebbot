cd /data

echo "Updating config.toml..."
wget -nv -O config.toml https://gitlab.gnome.org/World/twig/-/raw/main/hebbot/config.toml

echo "Updating template.md..."
wget -nv -O template.md https://gitlab.gnome.org/World/twig/-/raw/main/hebbot/template.md

echo "Updating update_config.sh..."
wget -nv -O template.md https://gitlab.gnome.org/World/twig/-/raw/main/hebbot/update_config.sh
