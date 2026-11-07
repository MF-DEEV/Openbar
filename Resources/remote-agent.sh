# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint
#
# Runs on a remote host Openbar reaches over SSH (issue #189); the Mac and the
# Windows island send this same file. Plain POSIX sh, so it runs under dash,
# bash, busybox ash or macOS sh, whatever the login shell is. Openbar starts
#
#   exec sh -c 'IFS= read -r s; eval "$(printf %s "$s" | base64 -d)"'
#
# and writes this file, base64 on one line, then the mode on the next line.
# Every answer is tagged lines ending with a lone "." line. Text that comes from
# the host (paths, records, transcripts) always travels as base64, so a line
# break or a space inside it can't end a field.
#
# Modes:
#   probe <socket> <app>  prepares ~/.openbar, says what the host has
#   read                  the Claude Code settings and the copy kept at setup
#   install               writes new settings, keeping the original once
#   remove                takes the hooks out again
#   agent                 answers requests until the connection ends
#
# Agent requests: snap (N host time, R records, J transcripts, T new bytes,
# G gone), paste, enter, focus, ping.

LC_ALL=C
export LC_ALL
umask 077

ob_b64() { base64 | tr -d '\n'; }
ob_unb64() { printf %s "$1" | base64 -d 2>/dev/null; }
ob_field() { printf %s "$1" | ob_b64; }
ob_end() { echo .; }

ob_settings() {
    ob_file="$HOME/.claude/settings.json"
    # Settings kept elsewhere and linked here are changed where they live.
    if [ -L "$ob_file" ]; then
        ob_real=$(readlink -f "$ob_file" 2>/dev/null) && [ -n "$ob_real" ] && ob_file=$ob_real
    fi
    ob_copy="$ob_file.openbar-remote.bak"
    ob_none="$ob_file.openbar-remote.none"
}

# The host's clock in milliseconds, so the island can put the registry's
# times on its own clock. Without %N (busybox, macOS) to the second.
ob_now() {
    ob_t=$(date +%s%N 2>/dev/null)
    case $ob_t in
    *N | '' | *[!0-9]*) echo "$(date +%s)000" ;;
    *) echo "${ob_t%??????}" ;;
    esac
}

ob_sum() { if [ -f "$1" ]; then cksum < "$1" | tr -s ' ' '-'; else echo none; fi; }

# When a process started, as Claude Code writes it in procStart: clock ticks
# since boot on Linux; elsewhere the process is only checked to be Claude.
ob_started() {
    if [ -r "/proc/$1/stat" ]; then
        ob_stat=$(cat "/proc/$1/stat" 2>/dev/null) || return 1
        ob_stat=${ob_stat##*) }
        # shellcheck disable=SC2086
        set -- $ob_stat
        [ $# -ge 20 ] || return 1
        shift 19
        printf '%s\n' "$1"
    else
        echo -
    fi
}

ob_live() {
    kill -0 "$1" 2>/dev/null || return 1
    ob_want=$(printf %s "$2" | sed -n 's/.*"procStart":"\([^"]*\)".*/\1/p')
    ob_have=$(ob_started "$1") || return 1
    if [ "$ob_have" = - ]; then
        ps -o command= -p "$1" 2>/dev/null | grep -qi claude
        return
    fi
    [ -z "$ob_want" ] || [ "$ob_want" = "$ob_have" ]
}

# The tmux pane a process runs in: sets ob_sock, ob_pane and ob_tty.
ob_tmux_of() {
    ob_sock=; ob_pane=; ob_tty=
    [ -r "/proc/$1/environ" ] || return 1
    ob_env=$(tr '\0' '\n' < "/proc/$1/environ" 2>/dev/null)
    ob_t=$(printf '%s\n' "$ob_env" | sed -n 's/^TMUX=//p' | head -n 1)
    ob_pane=$(printf '%s\n' "$ob_env" | sed -n 's/^TMUX_PANE=//p' | head -n 1)
    ob_sock=${ob_t%%,*}
    ob_tty=$(readlink "/proc/$1/fd/0" 2>/dev/null)
    case $ob_pane in %*) ;; *) return 1 ;; esac
    [ -n "$ob_sock" ] && [ -n "$ob_tty" ]
}

ob_records() {
    for ob_f in "$HOME"/.claude*/sessions/*.json "$HOME"/.config/claude/sessions/*.json; do
        [ -f "$ob_f" ] || continue
        ob_pid=${ob_f##*/}
        ob_pid=${ob_pid%.json}
        case $ob_pid in '' | *[!0-9]*) continue ;; esac
        ob_json=$(head -c 65536 "$ob_f" 2>/dev/null) || continue
        ob_live "$ob_pid" "$ob_json" || continue
        ob_home=${ob_f%/sessions/*}
        if ob_tmux_of "$ob_pid"; then
            ob_t="$(ob_field "$ob_sock") $ob_pane $(ob_field "$ob_tty")"
        else
            ob_t="- - -"
        fi
        echo "R $ob_pid $(ob_field "$ob_json") $(ob_field "$ob_home") $ob_t"
        ob_sid=$(printf %s "$ob_json" | sed -n 's/.*"sessionId":"\([0-9A-Za-z_-]*\)".*/\1/p')
        [ -n "$ob_sid" ] || continue
        for ob_j in "$ob_home"/projects/*/"$ob_sid".jsonl; do
            [ -f "$ob_j" ] && echo "J $ob_sid $(ob_field "$ob_j")"
            break
        done
    done
}

# Transcript bytes after an offset, at most a megabyte a time.
ob_tail() {
    ob_path=$(ob_unb64 "$2")
    case $1 in '' | *[!0-9]*) return ;; esac
    if [ ! -f "$ob_path" ]; then
        echo "G $2"
        return
    fi
    ob_size=$(wc -c < "$ob_path" | tr -d ' ')
    ob_from=$1
    # A file that got shorter was started over.
    [ "$ob_size" -lt "$ob_from" ] && ob_from=0
    [ "$ob_size" -gt "$ob_from" ] || return
    echo "T $2 $ob_from $(tail -c +"$((ob_from + 1))" "$ob_path" | head -c 1048576 | ob_b64)"
}

# Finds the session's pane and checks it still runs that session.
ob_target() {
    case $1 in '' | *[!0-9]*) return 1 ;; esac
    case $2 in '' | *[!0-9A-Za-z_-]*) return 1 ;; esac
    ob_rec=
    for ob_f in "$HOME"/.claude*/sessions/"$1".json "$HOME"/.config/claude/sessions/"$1".json; do
        [ -f "$ob_f" ] && grep -q "\"sessionId\":\"$2\"" "$ob_f" && ob_rec=$ob_f && break
    done
    [ -n "$ob_rec" ] || return 1
    ob_tmux_of "$1" || return 1
    command -v tmux > /dev/null 2>&1 || return 1
    ob_ptty=$(tmux -S "$ob_sock" display-message -p -t "$ob_pane" '#{pane_tty}' 2>/dev/null)
    [ "$ob_ptty" = "$ob_tty" ]
}

ob_waiting() { grep -q '"status":"waiting"' "$ob_rec"; }

ob_agent() {
    echo "HELLO 1"
    ob_end
    while IFS= read -r ob_line; do
        # shellcheck disable=SC2086
        set -- $ob_line
        ob_verb=$1
        [ $# -gt 0 ] && shift
        case $ob_verb in
        snap)
            echo "N $(ob_now)"
            ob_records
            while IFS= read -r ob_req && [ "$ob_req" != end ]; do
                # shellcheck disable=SC2086
                set -- $ob_req
                [ "$1" = t ] && ob_tail "$2" "$3"
            done
            ;;
        paste)
            if ! ob_target "$1" "$2"; then
                echo "F unreachable"
            elif ob_waiting; then
                echo "F waiting"
            else
                ob_buf="openbar-$$-$(date +%s)"
                if ob_unb64 "$3" | tmux -S "$ob_sock" load-buffer -b "$ob_buf" - 2>/dev/null &&
                    tmux -S "$ob_sock" paste-buffer -p -r -d -b "$ob_buf" -t "$ob_pane" 2>/dev/null; then
                    echo O
                else
                    echo "F unreachable"
                fi
            fi
            ;;
        enter)
            if ob_target "$1" "$2" && tmux -S "$ob_sock" send-keys -t "$ob_pane" Enter 2>/dev/null; then
                echo O
            else
                echo "F unreachable"
            fi
            ;;
        focus)
            if ob_target "$1" "$2"; then
                ob_win=$(tmux -S "$ob_sock" display-message -p -t "$ob_pane" '#{window_id}' 2>/dev/null)
                tmux -S "$ob_sock" select-window -t "$ob_win" 2>/dev/null
                tmux -S "$ob_sock" select-pane -t "$ob_pane" 2>/dev/null
                echo O
            else
                echo "F unreachable"
            fi
            ;;
        ping) echo O ;;
        *) echo "F unknown" ;;
        esac
        ob_end
    done
}

ob_probe() {
    case $1 in '' | */* | .*) echo "E name"; ob_end; return ;; esac
    mkdir -p "$HOME/.openbar" && chmod 700 "$HOME/.openbar"
    # sshd keeps a forwarded socket's file after the connection ends, and
    # refuses to listen on it again unless StreamLocalBindUnlink is on.
    rm -f "$HOME/.openbar/$1"
    echo "H $(ob_field "$HOME")"
    echo "U $(uname -s 2>/dev/null)"
    ob_client=none
    if command -v nc > /dev/null 2>&1 && nc -h 2>&1 | grep -q -- '-U'; then
        ob_client="nc $(ob_field "$(command -v nc)")"
    elif command -v socat > /dev/null 2>&1; then
        ob_client="socat $(ob_field "$(command -v socat)")"
    elif command -v python3 > /dev/null 2>&1; then
        ob_client="python3 $(ob_field "$(command -v python3)")"
    fi
    echo "C $ob_client"
    if command -v tmux > /dev/null 2>&1; then echo "M yes"; else echo "M no"; fi
    if command -v claude > /dev/null 2>&1 || [ -x "$HOME/.local/bin/claude" ] || [ -d "$HOME/.claude" ]; then
        echo "A yes"
    else
        echo "A no"
    fi
    ob_settings
    case $2 in '' | *[!0-9A-Za-z.-]*) ;; *)
        if [ -f "$ob_file" ] && grep -q "openbar-remote-hook $2\"" "$ob_file"; then echo "I yes"; else echo "I no"; fi ;;
    esac
    ob_end
}

ob_read() {
    ob_settings
    if [ -f "$ob_file" ]; then echo "F $(ob_b64 < "$ob_file")"; else echo "N"; fi
    if [ -f "$ob_copy" ]; then
        echo "B $(ob_b64 < "$ob_copy")"
    elif [ -f "$ob_none" ]; then
        echo "B -"
    fi
    echo "S $(ob_sum "$ob_file")"
    ob_end
}

# Replaces the file in one step, keeping its mode, unless it changed since
# Openbar read it.
ob_put() {
    ob_dir=${ob_file%/*}
    mkdir -p "$ob_dir" || return 1
    ob_tmp="$ob_dir/.settings.json.openbar-$$"
    ob_unb64 "$1" > "$ob_tmp" || { rm -f "$ob_tmp"; return 1; }
    if [ -f "$ob_file" ]; then
        chmod "$(stat -c %a "$ob_file" 2>/dev/null || stat -f %Lp "$ob_file" 2>/dev/null || echo 600)" "$ob_tmp" 2>/dev/null
    else
        chmod 600 "$ob_tmp"
    fi
    mv -f "$ob_tmp" "$ob_file"
}

ob_install() {
    ob_settings
    IFS= read -r ob_expected
    IFS= read -r ob_new
    if [ "$(ob_sum "$ob_file")" != "$ob_expected" ]; then echo "F changed"; ob_end; return; fi
    mkdir -p "${ob_file%/*}"
    # The settings as they were before the first setup, kept once.
    if [ ! -f "$ob_copy" ] && [ ! -f "$ob_none" ]; then
        if [ -f "$ob_file" ]; then cp -p "$ob_file" "$ob_copy" && chmod 600 "$ob_copy"; else : > "$ob_none"; fi
    fi
    if ob_put "$ob_new"; then echo O; else echo "F write"; fi
    ob_end
}

ob_remove() {
    ob_settings
    IFS= read -r ob_name
    IFS= read -r ob_expected
    IFS= read -r ob_how
    if [ "$(ob_sum "$ob_file")" != "$ob_expected" ]; then echo "F changed"; ob_end; return; fi
    case $ob_how in
    restore)
        # Nothing else changed: the file goes back byte for byte.
        if [ -f "$ob_copy" ]; then cp -p "$ob_copy" "$ob_file"; else rm -f "$ob_file"; fi
        ;;
    write)
        IFS= read -r ob_new
        ob_put "$ob_new" || { echo "F write"; ob_end; return; }
        ;;
    esac
    rm -f "$ob_copy" "$ob_none"
    case $ob_name in '' | */* | .*) ;; *) rm -f "$HOME/.openbar/$ob_name" ;; esac
    rmdir "$HOME/.openbar" 2>/dev/null
    echo O
    ob_end
}

IFS= read -r ob_mode_line
# shellcheck disable=SC2086
set -- $ob_mode_line
case $1 in
probe) ob_probe "$2" "$3" ;;
read) ob_read ;;
install) ob_install ;;
remove) ob_remove ;;
agent) ob_agent ;;
*) echo "E mode"; ob_end ;;
esac
