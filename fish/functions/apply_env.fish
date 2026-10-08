# fish shell function to apply environment variables from a bash/sh script
function apply_env --description "source a .sh script and apply its env changes to the current fish shell"
    argparse 'v/verbose' -- $argv
    or return

    # Check for input file
    if test -z "$argv[1]"
        echo "Usage: apply_env <path_to_script.sh>" >&2
        return 1
    end

    set -l script_path $argv[1]

    if not test -f "$script_path"
        echo "Error: File not found at '$script_path'" >&2
        return 1
    end

    # Blacklist of special/read-only variables to ignore
    set -l ignored_vars _ SHLVL PWD PS1 FROM_FISH_APPLY_ENV

    set -l env_bin (type -p env)
    set -l before ($env_bin -0 | string split0)
    set -l after_file (mktemp)
    or return

    # Run the target script without capturing its stdout or stderr.
    if set -q _flag_verbose
        echo "--- Sourcing '$script_path'... ---"
    end

    # Extract script arguments (everything after the script path)
    set -l script_args $argv[2..-1]

    env FROM_FISH_APPLY_ENV=1 bash -c '
        script_path=$1
        after_file=$2
        env_bin=$3
        shift 3
        source "$script_path" "$@"
        source_status=$?
        if [ "$source_status" -eq 0 ]; then
            "$env_bin" -0 > "$after_file"
        fi
        exit "$source_status"
    ' _ "$script_path" "$after_file" "$env_bin" $script_args
    set -l source_status $status

    if set -q _flag_verbose
        echo "--- End of script output ---"
    end

    if test "$source_status" -ne 0
        set -e FROM_FISH_APPLY_ENV
        rm "$after_file"
        return $source_status
    end

    set -l after (string split0 < "$after_file")
    rm "$after_file"

    if set -q _flag_verbose
        echo "Applying environment changes..."
    end

    # Compare NUL-delimited records without sorting or losing embedded newlines.
    set -l after_keys
    for line in $after
        set -l parts (string split -m 1 '=' -- "$line")
        set -l key $parts[1]
        set -l value $parts[2]
        set -a after_keys "$key"

        if contains -- "$line" $before
            continue
        end

        if contains -- "$key" $ignored_vars
            if set -q _flag_verbose
                echo "  Skipped (read-only): $key"
            end
            continue
        end

        set -gx -- "$key" "$value"
        if set -q _flag_verbose
            echo "  Applied: $key"
        end
    end

    # Remove only keys missing from the new environment.
    for line in $before
        set -l key (string split -m 1 '=' -- "$line")[1]

        if contains -- "$key" $ignored_vars; or contains -- "$key" $after_keys
            continue
        end

        set -e -- "$key"
        if set -q _flag_verbose
            echo "  Unset: $key"
        end
    end

    set -e FROM_FISH_APPLY_ENV

    if set -q _flag_verbose
        echo "Environment update complete."
    end
end
