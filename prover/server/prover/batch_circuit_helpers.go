package prover

import (
	merkletree "light/light-prover/merkle-tree"
	"light/light-prover/prover/poseidon"
	"math/big"

	"github.com/reilabs/gnark-lean-extractor/v2/abstractor"

	"github.com/consensys/gnark/frontend"
)

func createHashChain(api frontend.API, hashes []frontend.Variable) frontend.Variable {
	if len(hashes) == 0 {
		return frontend.Variable(0)
	}

	initialHash := hashes[0]
	return computeHashChain(api, initialHash, hashes)
}

func createTwoInputsHashChain(api frontend.API, hashesFirst []frontend.Variable, hashesSecond []frontend.Variable) frontend.Variable {
	if len(hashesFirst) == 0 {
		return abstractor.Call(api, poseidon.Poseidon2{In1: hashesFirst[0], In2: hashesSecond[0]})
	}

	hashChain := abstractor.Call(api, poseidon.Poseidon2{In1: hashesFirst[0], In2: hashesSecond[0]})
	for i := 1; i < len(hashesFirst); i++ {
		hashChain = abstractor.Call(api, poseidon.Poseidon3{In1: hashChain, In2: hashesFirst[i], In3: hashesSecond[i]})
	}
	return hashChain
}

func computeHashChain(api frontend.API, initialHash frontend.Variable, hashes []frontend.Variable) frontend.Variable {
	hashChain := initialHash

	for i := 1; i < len(hashes); i++ {
		hashChain = abstractor.Call(api, poseidon.Poseidon2{In1: hashChain, In2: hashes[i]})
	}

	return hashChain
}

// getZeroValue returns the zero value for a given tree level
func getZeroValue(level int) frontend.Variable {
	return frontend.Variable(new(big.Int).SetBytes(merkletree.ZERO_BYTES[level][:]))
}
