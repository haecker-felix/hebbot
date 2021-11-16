cd /data

echo "Updating config.toml..."
wget -nv -O config.toml https://mydomain.com/hebbot/config.toml

echo "Updating report_template.md..."
wget -nv -O report_template.md https://mydomain.com/hebbot/report_template.md

echo "Updating section_template.md..."
wget -nv -O section_template.md https://mydomain.com/hebbot/section_template.md

echo "Updating template.md..."
wget -nv -O project_template.md https://mydomain.com/hebbot/project_template.md

echo "Updating update_config.sh..."
wget -nv -O update_config.sh https://mydomain.com/hebbot/update_config.sh
