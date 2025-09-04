# fish shell function to apply environment variables from a bash/sh script
function apply_env --description "source a .sh script and apply its env changes to the current fish shell"
    argparse 'v/verbose' -- $argv
    or return

    # Check for input file
    if test -z "$argv[1]"
        echo "Usage: apply_env <path_to_script.sh>"
        return 1
    end

    set script_path $argv[1]

    if not test -f "$script_path"
        echo "Error: File not found at '$script_path'"
        return 1
    end

    # Blacklist of special/read-only variables to ignore
    set -l ignored_vars _ SHLVL PWD PS1

    set before_file (mktemp)
    set after_file (mktemp)

    # 1. Get environment snapshot before
    env | sort > "$before_file"

    # 2. Run the target script.
    if set -q _flag_verbose
        echo "--- Sourcing '$script_path'... ---"
    end

    # We execute two commands in a bash subshell:
    # 1. `source ...`: This sources the script.
    # 2. `env`: This prints the environment *after* sourcing.
    # The > "$after_file" at the end captures only the output of the *last* command (env).
    # Extract script arguments (everything after the script path)
    set -l script_args $argv[2..-1]
    
    env FROM_FISH_APPLY_ENV=1 bash -c "source '$(string escape -- $script_path)' $(string escape -- $script_args | string join ' '); env > \"$after_file\""

    if set -q _flag_verbose
        echo "--- End of script output ---"
    end

    # Sort the captured environment file
    sort -o "$after_file" "$after_file"

    # 3. Calculate differences
    set added_or_changed (comm -13 "$before_file" "$after_file")
    set removed (comm -23 "$before_file" "$after_file")

    rm "$before_file" "$after_file"

    # 4. Get a list of keys that were added/changed to avoid incorrectly unsetting them
    set changed_keys
    for line in $added_or_changed
        if test -z "$line"; continue; end
        set -l key (string split -m 1 '=' -- "$line")[1]
        set changed_keys $changed_keys $key
    end

    if set -q _flag_verbose
        echo "Applying environment changes..."
    end

    # 5. Apply added or changed variables
    for line in $added_or_changed
        if test -z "$line"; continue; end
        set -l key (string split -m 1 '=' -- "$line")[1]
        set -l value (string split -m 1 '=' -- "$line")[2]

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

    # 6. Handle truly unset variables
    for line in $removed
        if test -z "$line"; continue; end
        set -l key (string split -m 1 '=' -- "$line")[1]

        # Ignore blacklisted vars and vars that were changed (not unset)
        if contains -- "$key" $ignored_vars; or contains -- "$key" $changed_keys
            continue
        end

        set -e -- "$key"
        if set -q _flag_verbose
            echo "  Unset: $key"
        end
    end

    if set -q _flag_verbose
        echo "Environment update complete."
    end
end
