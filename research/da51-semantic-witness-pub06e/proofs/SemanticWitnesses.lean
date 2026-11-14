import Lean

namespace PUB06E09B

-- Alpha-renaming
def alphaA (n : Nat) : Nat := n + 1
def alphaB (x : Nat) : Nat := x + 1

theorem alpha_equivalent :
    ∀ n : Nat, alphaA n = alphaB n := by
  intro n
  rfl

-- Different syntax, equivalent behavior
def addA (n : Nat) : Nat := n + 1
def addB (n : Nat) : Nat := n + 0 + 1

theorem addition_equivalent :
    ∀ n : Nat, addA n = addB n := by
  intro n
  simp [addA, addB]

-- Order changes behavior
def orderA (n : Nat) : Nat := n - 1
def orderB (n : Nat) : Nat := 1 - n

theorem order_difference :
    orderA 10 ≠ orderB 10 := by
  decide

-- Different string values
def stringA : String := "hello"
def stringB : String := "world"

theorem string_difference :
    stringA ≠ stringB := by
  decide

end PUB06E09B
