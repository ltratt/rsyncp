# rsyncp: a progress wrapper for rsync

`rsync` is a wonderful tool, but does not provide any notion of "how much has
been done / how much is left"? `rsyncp` does just that, providing the estimated
percentage completed, estimated time remaining, and files sent to (green) /
deleted on (red) the destination. It does this by comparing the current `rsync`
run to the previous run. By definition, percentages and time remaining only
appear from the second run onwards.

In general, to use `rsyncp` you need only change:

```sh
rsync -arg1 -arg2 src1 src2 dst
```

to:

```sh
rsyncp -- -arg1 -arg2 src1 src2 dst
```

The arguments to the right of the `--` are used to form a "cookie" that
identifies this command. If you use multiple different `rsync` invocations,
`rsyncp` treats them distinctly, giving appropriate time estimates for each.


## Usage

`rsyncp` has the following command-line format:

```
rsyncp [-h] [-c <cookie> | -x <str> [...-x <str>]] [-i] [-r <rsync_name>] -- <rsync_arg_1> [... <rsync_arg_n>]
```

where:

* `-c` and `-x` control cookie construction. These two flags are mutually exclusive.
  See [cookie construction](#cookie_construction) for more details.
* `-i` prints out version number info (etc.) and then immediately exits.
* `-r` specifies the name of the rsync binary. Defaults to `rsync`.


## Cookie construction

By default, `rsyncp` uses all `rsync` arguments to form a cookie.

The user can pass their own cookie with `-c <cookie>`, overriding the default
cookie entirely. It is guaranteed that user cookies can't clash with `rsyncp`'s
cookies, so users can use whatever format they want for `<cookie>`.

Alternatively, users can exclude an `rsync` argument from consideration in a
cookie with `-x <str>`. For example, `rsyncp -x "--exclude-from=/tmp/abc" --
--exclude-from="/tmp/abc" src dst` will form a cookie from `src` and `dst`
alone.


## State storage

`rsyncp` stores state in:

```
${XDG_STATE_HOME:-$HOME/.local/state}/rsyncp/
```

Users can at any point remove this directory or any subset of its contents.


## Limitations

`rsyncp` estimates progress from the number of files `rsync` reports it has to
check. This means that the time remaining is only accurate when repeated
invocations have similar file counts and similar per-file costs.
