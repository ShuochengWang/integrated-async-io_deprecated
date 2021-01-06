#!/bin/bash
# compile server and client
make && \
cd client && cargo build --release && cd ..

# run server
cd bin
./app &
cd ..

sleep 1

# run clients
./client/target/release/client &
./client/target/release/client &
./client/target/release/client

sleep 2
# kill server and clients
for pid in $(/bin/ps | grep "client" | awk '{print $1}'); do kill -9 $pid; done
for pid in $(/bin/ps | grep "app" | awk '{print $1}'); do kill -9 $pid; done