import SemanticWitnesses

namespace PUB06E10C

open PUB06E09B

/-
The elaborator must accept each exact theorem
at its declared semantic proposition.
-/

theorem alpha_binding :
    ∀ n : Nat, alphaA n = alphaB n :=
  PUB06E09B.alpha_equivalent

theorem addition_binding :
    ∀ n : Nat, addA n = addB n :=
  PUB06E09B.addition_equivalent

theorem order_binding :
    orderA 10 ≠ orderB 10 :=
  PUB06E09B.order_difference

theorem string_binding :
    stringA ≠ stringB :=
  PUB06E09B.string_difference

end PUB06E10C
