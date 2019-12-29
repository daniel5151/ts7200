# should be called from the `ts7200` directory
# i.e: `gdb-multiarch -x ./reference/iotest.gdbinit`
#
# Unless by sheer luck you happen to put your source files in the exact same
# directory as I do, you'll need to manually specify which directory
# iotest.elf's source files are in.
# An alternative (and easier approach) is to use a program you compiled yourself

file reference/iotest.elf

# set debug remote 1
target remote localhost:9001
# tui enable
