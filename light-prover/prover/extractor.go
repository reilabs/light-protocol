package prover

import (
	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/reilabs/gnark-lean-extractor/v2/extractor"
)

func ExtractLean(treeDepth uint32, numberOfCompressedAccounts uint32) (string, error) {
	// Not checking for numberOfCompressedAccounts === 0 or treeDepth === 0

	// Initialising MerkleProofs slice with correct dimensions
	inclusionInPathElements := make([][]frontend.Variable, numberOfCompressedAccounts)
	nonInclusionInPathElements := make([][]frontend.Variable, numberOfCompressedAccounts)

	for i := 0; i < int(numberOfCompressedAccounts); i++ {
		inclusionInPathElements[i] = make([]frontend.Variable, treeDepth)
		nonInclusionInPathElements[i] = make([]frontend.Variable, treeDepth)
	}

	inclusionCircuit := InclusionCircuit{
		Depth:                      treeDepth,
		NumberOfCompressedAccounts: numberOfCompressedAccounts,
		Roots:                      make([]frontend.Variable, numberOfCompressedAccounts),
		Leaves:                     make([]frontend.Variable, numberOfCompressedAccounts),
		InPathIndices:              make([]frontend.Variable, numberOfCompressedAccounts),
		InPathElements:             inclusionInPathElements,
	}

	nonInclusionCircuit := NonInclusionCircuit{
		Depth:                      treeDepth,
		NumberOfCompressedAccounts: numberOfCompressedAccounts,
		Roots:                      make([]frontend.Variable, numberOfCompressedAccounts),
		Values:                     make([]frontend.Variable, numberOfCompressedAccounts),
		LeafLowerRangeValues:       make([]frontend.Variable, numberOfCompressedAccounts),
		LeafHigherRangeValues:      make([]frontend.Variable, numberOfCompressedAccounts),
		NextIndices:                make([]frontend.Variable, numberOfCompressedAccounts),
		InPathIndices:              make([]frontend.Variable, numberOfCompressedAccounts),
		InPathElements:             nonInclusionInPathElements,
	}

	combinedCircuit := CombinedCircuit{
		Inclusion:    inclusionCircuit,
		NonInclusion: nonInclusionCircuit,
	}

	return extractor.ExtractCircuits("LightProver", ecc.BN254, &inclusionCircuit, &nonInclusionCircuit, &combinedCircuit)
}
