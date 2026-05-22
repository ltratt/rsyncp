# rsyncp: an ETA wrapper for rsync

`rsync` is a wonderful tool, but does not provide any notion of "how much has
been done / how much is left"? `rsyncp` does just that, providing ETAs and
percentage completed for repeated `rsync` invocations: it remembers the
previous run's elapsed time and total files and reuses those on the next run.
On the first run, when no such information is available, it prints out the
number of files `rsync` has found so far, so you know `rsync` is running.

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
`rsyncp` treats them distinctly, giving appropriate ETAs for each.


## Usage

`rsyncp` has the following command-line format:

```
rsyncp [-h] [-i] [-x <str> [...-x <str>] | -c <cookie>] -- <rsync_arg_1> [... <rsync_arg_n>]
```

where:

* `-i` prints out version number info (etc.) and then immediately exits.
* `-c` and `-x` control cookie construction. These two flags are mutually exclusive.


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
check. This means that the ETA is only accurate when repeated invocations have
similar file counts and similar per-file costs.
