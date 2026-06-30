function _tide_remove_unusable_items
    set -l left_prompt_items (for item in $tide_left_prompt_items
        switch $item
            case virtual_env
                echo python
            case chruby
                echo ruby
            case '*'
                echo $item
        end
    end)
    set -l right_prompt_items (for item in $tide_right_prompt_items
        switch $item
            case virtual_env
                echo python
            case chruby
                echo ruby
            case '*'
                echo $item
        end
    end)

    # Remove tool-specific items for tools the machine doesn't have installed
    set -l removed_items
    for item in aws bun crystal direnv distrobox docker elixir gcloud git go java kubectl nix_shell node php pulumi python ruby rustc terraform toolbox zig
        contains $item $left_prompt_items $right_prompt_items || continue

        set -l cli_names $item
        switch $item
            case distrobox # there is no 'distrobox' command inside the container
                set cli_names distrobox-export # 'distrobox-export' and 'distrobox-host-exec' are available
            case nix_shell
                set cli_names nix nix-shell
            case python
                set cli_names python python3
        end
        type --query $cli_names || set -a removed_items $item
    end

    set -U _tide_left_items (for item in $left_prompt_items
        contains $item $removed_items || echo $item
    end)
    set -U _tide_right_items (for item in $right_prompt_items
        contains $item $removed_items || echo $item
    end)
end
