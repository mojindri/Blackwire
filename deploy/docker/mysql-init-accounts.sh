#!/bin/sh
set -eu

runtime_password=$(cat /run/secrets/mysql_runtime_password)
ui_password=$(cat /run/secrets/mysql_ui_password)
migrator_password=$(cat /run/secrets/mysql_migrator_password)

for password in "$runtime_password" "$ui_password" "$migrator_password"; do
    case "$password" in
        *"'"*|*"\\"*|*"
"*) echo "Blackwire MySQL passwords must not contain quotes, backslashes, or newlines" >&2; exit 1 ;;
    esac
done

mysql --protocol=socket -uroot -p"$(cat /run/secrets/mysql_root_password)" <<SQL
CREATE USER IF NOT EXISTS 'blackwire_runtime'@'%' IDENTIFIED BY '${runtime_password}';
CREATE USER IF NOT EXISTS 'blackwire_ui'@'%' IDENTIFIED BY '${ui_password}';
CREATE USER IF NOT EXISTS 'blackwire_migrator'@'%' IDENTIFIED BY '${migrator_password}';
GRANT SELECT, INSERT, UPDATE, DELETE ON blackwire.* TO 'blackwire_runtime'@'%';
GRANT SELECT, INSERT, UPDATE, DELETE ON blackwire.* TO 'blackwire_ui'@'%';
GRANT ALL PRIVILEGES ON blackwire.* TO 'blackwire_migrator'@'%';
FLUSH PRIVILEGES;
SQL
