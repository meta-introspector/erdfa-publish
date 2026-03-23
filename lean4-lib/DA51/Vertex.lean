import DA51.CborVal
import DA51.Encode
import DA51.Monster
open DA51.CborVal CborVal DA51.Encode DA51.Monster

/-! DA51.Vertex: Vertex Operator Algebra instance for the DA51 Monster element

A VOA is (V, Y, 1, ω) where:
  V = state space (our CborVal terms)
  Y = vertex operator (state → formal series of operators)
  1 = vacuum (cnull)
  ω = conformal vector (the Monster element shard)

We encode the VOA structure using HVertexOperator coefficients
indexed by SSP primes, following Borcherds 1986.

Reference: Mathlib.Algebra.Vertex.HVertexOperator
-/

namespace DA51.Vertex

-- The 15 SSP primes as our grading group Γ = Z^15
-- Each prime p_i gives a Z-grading via the coordinate in Z/p_i^e_i

/-- A vertex operator coefficient: maps grade n to a linear map on CborVal -/
structure VOACoeff where
  grade : Nat        -- which SSP prime index (0..14)
  index : Nat        -- coefficient index in the formal series
  value : CborVal    -- the coefficient data
deriving Repr

/-- A vertex operator: formal series of coefficients -/
structure VertexOp where
  state : CborVal           -- the state this operator is associated with
  coeffs : List VOACoeff    -- Y(state, z) = Σ coeffs[n] z^{-n-1}
deriving Repr

/-- The VOA structure -/
structure VOA where
  vacuum   : CborVal              -- |0⟩
  conformal : CborVal             -- ω (Virasoro element)
  states   : List CborVal         -- state space basis
  vertex   : List VertexOp        -- vertex operators Y(v, z) for each state
  rank     : Nat                  -- central charge numerator (c = rank/1)
deriving Repr

/-- Build a VOA from a MonsterElement: each coordinate becomes a vertex operator -/
def fromMonster (me : MonsterElement) : VOA :=
  let states := me.coords.filterMap fun (kind, prime, coord) =>
    if coord > 0 then some (cmap [
      ((ctext "kind"), (ctext kind)),
      ((ctext "prime"), (cnat prime)),
      ((ctext "coord"), (cnat coord))
    ]) else none
  let vertex := me.coords.filterMap fun (kind, prime, coord) =>
    if coord > 0 then
      let coeffs := (List.range (min coord 8)).map fun n =>
        { grade := prime, index := n, value := cnat (coord / (n + 1)) : VOACoeff }
      some { state := ctext kind, coeffs := coeffs : VertexOp }
    else none
  { vacuum := cnull
    conformal := toCborVal me
    states := states
    vertex := vertex
    rank := grade me  -- central charge = grade = number of active primes
  }

/-- Export VOA as DA51 CBOR -/
def VOA.toCborVal (voa : VOA) : CborVal :=
  ctag 55889 (cmap [
    ((ctext "type"), (ctext "vertex-operator-algebra")),
    ((ctext "rank"), (cnat voa.rank)),
    ((ctext "vacuum"), voa.vacuum),
    ((ctext "num_states"), (cnat voa.states.length)),
    ((ctext "num_vertex_ops"), (cnat voa.vertex.length)),
    ((ctext "states"), (carray voa.states)),
    ((ctext "vertex_ops"), (carray (voa.vertex.map fun vo =>
      cmap [
        ((ctext "state"), vo.state),
        ((ctext "num_coeffs"), (cnat vo.coeffs.length)),
        ((ctext "coeffs"), (carray (vo.coeffs.map fun c =>
          cmap [
            ((ctext "grade"), (cnat c.grade)),
            ((ctext "index"), (cnat c.index)),
            ((ctext "value"), c.value)
          ])))
      ])))
  ])

-- Build the VOA from our combined Monster element
def monsterVOA : VOA := fromMonster combinedElement

def main : IO Unit := do
  let voa := monsterVOA
  IO.println s!"Monster VOA:"
  IO.println s!"  rank (central charge): {voa.rank}"
  IO.println s!"  vacuum: {repr voa.vacuum}"
  IO.println s!"  states: {voa.states.length}"
  IO.println s!"  vertex operators: {voa.vertex.length}"
  for vo in voa.vertex do
    IO.println s!"    Y({repr vo.state}, z): {vo.coeffs.length} coefficients"
  let shard := voa.toCborVal
  let bytes := encode shard
  IO.FS.writeBinFile "monster_voa.cbor" bytes
  IO.println s!"\nWrote monster_voa.cbor ({bytes.size} bytes)"

end DA51.Vertex

def main := DA51.Vertex.main
