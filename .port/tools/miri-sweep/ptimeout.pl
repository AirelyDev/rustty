#!/usr/bin/perl
use strict; use warnings; use POSIX ();
my $secs = shift @ARGV;
my $pid = fork();
if (!defined $pid) { die "fork: $!"; }
if ($pid == 0) { POSIX::setpgid(0,0); exec @ARGV or die "exec: $!"; }
POSIX::setpgid($pid,$pid);
$SIG{ALRM} = sub { kill('KILL', -$pid); waitpid($pid,0); exit 137; };
alarm $secs;
waitpid($pid, 0);
exit($? >> 8);
