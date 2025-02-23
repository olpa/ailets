set -eux

this=$(dirname $0)
echo $this
$this/../run_actor.py $this/../target/wasm32-unknown-unknown/debug/cat.wasm in:="text is here" out:=@x.txt out:log=-
echo "Checking content of x.txt..."
cat x.txt
echo
