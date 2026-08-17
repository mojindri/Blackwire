#!/bin/sh
set -eu

mysql --protocol=tcp -hmysql -uroot -p"$(cat /run/secrets/mysql_root_password)" <<'SQL'
GRANT SELECT ON blackwire.* TO 'blackwire_runtime'@'%';
GRANT INSERT, UPDATE ON blackwire.configuration_state TO 'blackwire_runtime'@'%';
GRANT SELECT, INSERT, UPDATE ON blackwire.runtime_instances TO 'blackwire_runtime'@'%';
GRANT SELECT, INSERT, UPDATE ON blackwire.user_traffic TO 'blackwire_runtime'@'%';
GRANT SELECT, INSERT, UPDATE ON blackwire.inbound_traffic TO 'blackwire_runtime'@'%';
GRANT SELECT, INSERT, UPDATE ON blackwire.enforcement_state TO 'blackwire_runtime'@'%';
GRANT DELETE ON blackwire.configuration_revisions TO 'blackwire_runtime'@'%';

GRANT SELECT, INSERT, UPDATE, DELETE ON blackwire.* TO 'blackwire_ui'@'%';
FLUSH PRIVILEGES;
SQL
