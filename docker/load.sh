#!/bin/sh
# Generates load on the test servers so every rutop view has something to show.
#
#   load.sh [hosts] [duration_s] [oltp_workers]
#   hosts: comma separated, e.g. "mysql,mariadb"  (default: mysql)
#
# Runs inside the "load" service of docker-compose.test.yml (see load.ps1).
#
# What it produces (per host):
#   - OLTP workers (user rutop_app, db shop): SELECT/INSERT/UPDATE/DELETE/REPLACE
#     -> header qps, Se/In/Up/De, command summary (c), status counters (S)
#   - reporter (user rutop_report, db analytics): ~10s scan query + SLEEP(15)
#     -> long running threads, slow queries, f/e (full query / EXPLAIN)
#   - row lock contention on shop.counters -> threads waiting for locks, InnoDB status (I)
#   - idle connections (Command "Sleep") -> i (idle toggle)
# Killed connections (k/K in rutop) reconnect after a second.

HOSTS=${1:-mysql}
DURATION=${2:-300}
WORKERS=${3:-4}
export MYSQL_PWD=${MYSQL_PWD:-rutop}   # same password for root and the test users

sql() { # sql host user db [mysql args...]
    h=$1; u=$2; d=$3; shift 3
    mysql -h"$h" -u"$u" -N -B "$d" "$@"
}

setup() {
    h=$1
    echo "[$h] setting up schema and users"
    until mysql -h"$h" -uroot -e "SELECT 1" >/dev/null 2>&1; do sleep 1; done
    mysql -h"$h" -uroot <<'SQL'
CREATE DATABASE IF NOT EXISTS shop;
CREATE DATABASE IF NOT EXISTS analytics;
CREATE TABLE IF NOT EXISTS shop.orders (
    id INT AUTO_INCREMENT PRIMARY KEY,
    customer INT NOT NULL,
    amount DECIMAL(10,2) NOT NULL,
    status VARCHAR(16) NOT NULL,
    created TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    KEY (customer), KEY (status)
) ENGINE=InnoDB;
CREATE TABLE IF NOT EXISTS shop.counters (id INT PRIMARY KEY, v BIGINT NOT NULL) ENGINE=InnoDB;
INSERT IGNORE INTO shop.counters VALUES (1, 0);
CREATE TABLE IF NOT EXISTS analytics.seq (n INT PRIMARY KEY) ENGINE=InnoDB;
INSERT IGNORE INTO analytics.seq (n)
    SELECT a.d + 10 * b.d + 100 * c.d + 1
    FROM (SELECT 0 d UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
          UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) a,
         (SELECT 0 d UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
          UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) b,
         (SELECT 0 d UNION SELECT 1 UNION SELECT 2 UNION SELECT 3 UNION SELECT 4
          UNION SELECT 5 UNION SELECT 6 UNION SELECT 7 UNION SELECT 8 UNION SELECT 9) c;
CREATE USER IF NOT EXISTS 'rutop_app'@'%' IDENTIFIED BY 'rutop';
CREATE USER IF NOT EXISTS 'rutop_report'@'%' IDENTIFIED BY 'rutop';
GRANT SELECT, INSERT, UPDATE, DELETE ON shop.* TO 'rutop_app'@'%';
GRANT SELECT ON analytics.* TO 'rutop_report'@'%';
GRANT SELECT ON shop.* TO 'rutop_report'@'%';
SQL
}

# One long-lived connection that gets a steady stream of statements.
oltp_stream() {
    i=0
    while :; do
        echo "SET @c = FLOOR(RAND() * 1000);"
        echo "INSERT INTO orders (customer, amount, status) VALUES (@c, ROUND(RAND() * 100, 2), 'new');"
        echo "SELECT id, amount, status FROM orders WHERE customer = @c ORDER BY id DESC LIMIT 10;"
        echo "SELECT id, amount FROM orders WHERE customer = @c AND status = 'new' LIMIT 3;"
        echo "UPDATE orders SET status = 'paid' WHERE customer = @c AND status = 'new' LIMIT 5;"
        if [ $((i % 4)) -eq 0 ]; then
            echo "SELECT status, COUNT(*), SUM(amount) FROM orders GROUP BY status;"
        fi
        if [ $((i % 3)) -eq 0 ]; then
            echo "DELETE FROM orders WHERE customer = @c AND status = 'paid' LIMIT 2;"
        fi
        if [ $((i % 10)) -eq 0 ]; then
            echo "REPLACE INTO orders (id, customer, amount, status) VALUES (1, 0, 1.00, 'replaced');"
        fi
        i=$((i + 1))
        sleep 0.05
    done
}

oltp() { # host
    while :; do
        oltp_stream | sql "$1" rutop_app shop >/dev/null 2>&1
        sleep 1
    done
}

reporter() { # host
    while :; do
        # ~10 s, EXPLAIN shows a range scan on analytics.seq
        sql "$1" rutop_report analytics -e \
            "SELECT COUNT(*) AS report_rows FROM (SELECT s.n, SLEEP(0.02) AS z FROM seq s WHERE s.n <= 500) x" \
            >/dev/null 2>&1
        # longer than long_query_time (10 s) -> counts as a slow query
        sql "$1" rutop_report analytics -e "SELECT SLEEP(15) AS nightly_report" >/dev/null 2>&1
        sleep 3
    done
}

locker() { # host: holds a row lock for a few seconds
    while :; do
        sql "$1" rutop_app shop -e \
            "BEGIN; SELECT v FROM counters WHERE id = 1 FOR UPDATE; DO SLEEP(6); UPDATE counters SET v = v + 1 WHERE id = 1; COMMIT;" \
            >/dev/null 2>&1
        sleep 2
    done
}

waiter() { # host: blocks on the locked row
    while :; do
        sql "$1" rutop_app shop -e "UPDATE counters SET v = v + 1 WHERE id = 1" >/dev/null 2>&1
        sleep 1
    done
}

idle() { # host: connection that just sits there (Command = Sleep)
    while :; do
        sleep 100000 | sql "$1" rutop_app shop >/dev/null 2>&1
        sleep 1
    done
}

cleanup() {
    trap - INT EXIT
    trap '' TERM          # don't kill ourselves, only the workers
    echo "stopping load"
    kill 0 2>/dev/null
    exit 0
}
trap cleanup INT TERM EXIT

for h in $(echo "$HOSTS" | tr ',' ' '); do
    setup "$h" || exit 1
    w=1
    while [ "$w" -le "$WORKERS" ]; do
        oltp "$h" &
        w=$((w + 1))
    done
    reporter "$h" &
    locker "$h" &
    waiter "$h" &
    idle "$h" &
    idle "$h" &
    echo "[$h] started $WORKERS OLTP workers, reporter, lock contention, 2 idle connections"
done

echo "generating load for ${DURATION}s (Ctrl-C to stop)"
sleep "$DURATION"
