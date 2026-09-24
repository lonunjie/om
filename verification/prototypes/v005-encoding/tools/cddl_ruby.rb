# Feasibility prototype for V-005. Not product code. Not for production.
# Drives the reference Ruby `cddl` gem (0.12.14) as a library so one process can
# validate many instances / generate many examples.
#   ruby cddl_ruby.rb validate SPEC.cddl FILE.cbor...   -> JSON {basename: true|false}
#   ruby cddl_ruby.rb generate SPEC.cddl N OUTDIR       -> writes OUTDIR/gen-<i>.cbor
require 'set'
require 'cddl'
require 'cbor-pure'
require 'json'

mode, spec = ARGV[0], ARGV[1]
parser = CDDL::Parser.new(File.read(spec, encoding: 'utf-8'))
case mode
when 'validate'
  out = {}
  ARGV[2..].each do |f|
    inst = CBOR.decode(File.binread(f))
    out[File.basename(f)] = !!parser.validate(inst, false)
  end
  puts JSON.generate(out)
when 'generate'
  n, dir = ARGV[2].to_i, ARGV[3]
  srand(20260924) # deterministic example set
  n.times do |i|
    File.binwrite(File.join(dir, format('gen-%03d.cbor', i)), CBOR.encode(parser.generate))
  end
else
  abort 'usage: cddl_ruby.rb validate|generate ...'
end
