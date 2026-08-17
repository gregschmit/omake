# Minimal FindBin shim so the GNU make test runner works without the perl-FindBin package installed.
# The runner only uses $FindBin::Bin (the directory of the running script).
package FindBin;
use strict;
use warnings;
use Cwd qw(abs_path);
use File::Basename qw(dirname);

our ($Bin, $RealBin, $Script, $RealScript);
$Script = $RealScript = $0;
$Bin = $RealBin = dirname(abs_path($0));

1;
