
readonly TARGET_HOST=pi@192.168.100.249
readonly TARGET_PATH=/home/pi/prisma-engines/target/release
readonly SOURCE_PATH=$(pwd) 

export PRISMA_QUERY_ENGINE_BINARY=/home/pi/prisma-engines/target/release/query-engine
export PRISMA_MIGRATION_ENGINE_BINARY=/home/pi/prisma-engines/target/release/migration-engine
export PRISMA_INTROSPECTION_ENGINE_BINARY=/home/pi/prisma-engines/target/release/introspection-engine
export PRISMA_FMT_BINARY=/home/pi/prisma-engines/target/release/prisma-fmt

rsync -aP ${SOURCE_PATH} ${TARGET_HOST}:${TARGET_PATH}

ssh -t ${TARGET_HOST} su -c "/home/pi/prisma-engines/target/release"

cargo build --release


prisma generate

npx prisma migrate dev