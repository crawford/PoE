#!/usr/bin/env bash

# Copyright 2025 Alex Crawford
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

HOST=poe.h.jcraw.net

set -e

function trim-ending() {
    echo -n "${@}" \
        | head --lines -1 \
        | tr -d '\r'
}

function run() {
    trim-ending "$(echo $@ | nc -N $HOST 23)"
}

start=$(run prog addr)

function runapp() {
    trim-ending "$(make --silent --directory $1 run HOST=$HOST ADDR=$start)"
}

function check() {
    output=$(runapp $1)
    expect=$2
    if [[ "${output}" != "${expect}" ]]
    then
        echo "Mismatch when running '${1}'!"
        echo -e "Actual:\n  ${output}"
        echo -e "Expected:\n  ${expect}"
        exit 1
    fi
}

check call "> Return value (may not be valid): 0x00000000"
check return "> Return value (may not be valid): 0xABCDEF01"
