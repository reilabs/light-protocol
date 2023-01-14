use groth16_solana::groth16::Groth16Verifyingkey;

pub const VERIFYINGKEY: Groth16Verifyingkey =  Groth16Verifyingkey {
	nr_pubinputs: 16,

	vk_alpha_g1: [
		45,77,154,167,227,2,217,223,65,116,157,85,7,148,157,5,219,234,51,251,177,108,100,59,34,245,153,162,190,109,242,226,
		20,190,221,80,60,55,206,176,97,216,236,96,32,159,227,69,206,137,131,10,25,35,3,1,240,118,202,255,0,77,25,38,
	],

	vk_beta_g2: [
		9,103,3,47,203,247,118,209,175,201,133,248,136,119,241,130,211,132,128,166,83,242,222,202,169,121,76,188,59,243,6,12,
		14,24,120,71,173,76,121,131,116,208,214,115,43,245,1,132,125,214,139,192,224,113,36,30,2,19,188,127,193,61,183,171,
		48,76,251,209,224,138,112,74,153,245,232,71,217,63,140,60,170,253,222,196,107,122,13,55,157,166,154,77,17,35,70,167,
		23,57,193,177,164,87,168,199,49,49,35,210,77,47,145,146,248,150,183,198,62,234,5,169,213,127,6,84,122,208,206,200,
	],

	vk_gamme_g2: [
		25,142,147,147,146,13,72,58,114,96,191,183,49,251,93,37,241,170,73,51,53,169,231,18,151,228,133,183,174,243,18,194,
		24,0,222,239,18,31,30,118,66,106,0,102,94,92,68,121,103,67,34,212,247,94,218,221,70,222,189,92,217,146,246,237,
		9,6,137,208,88,95,240,117,236,158,153,173,105,12,51,149,188,75,49,51,112,179,142,243,85,172,218,220,209,34,151,91,
		18,200,94,165,219,140,109,235,74,171,113,128,141,203,64,143,227,209,231,105,12,67,211,123,76,230,204,1,102,250,125,170,
	],

	vk_delta_g2: [
		25,142,147,147,146,13,72,58,114,96,191,183,49,251,93,37,241,170,73,51,53,169,231,18,151,228,133,183,174,243,18,194,
		24,0,222,239,18,31,30,118,66,106,0,102,94,92,68,121,103,67,34,212,247,94,218,221,70,222,189,92,217,146,246,237,
		9,6,137,208,88,95,240,117,236,158,153,173,105,12,51,149,188,75,49,51,112,179,142,243,85,172,218,220,209,34,151,91,
		18,200,94,165,219,140,109,235,74,171,113,128,141,203,64,143,227,209,231,105,12,67,211,123,76,230,204,1,102,250,125,170,
	],

	vk_ic: &[
		[
			21,142,49,238,172,173,115,71,14,169,225,56,49,183,25,44,140,33,82,74,140,221,80,229,40,58,29,66,174,61,215,140,
			19,148,189,224,147,119,172,96,144,178,254,62,99,194,72,66,236,242,44,175,37,107,111,252,22,129,193,142,154,218,242,112,
		],
		[
			15,25,31,72,223,128,96,130,134,216,130,76,19,16,110,126,235,215,254,222,170,42,137,244,89,14,20,37,119,123,17,67,
			26,82,160,32,165,96,66,25,36,6,173,86,170,248,244,240,8,152,9,1,92,218,146,117,78,97,225,198,4,200,134,126,
		],
		[
			26,233,251,16,90,237,97,34,216,29,107,126,93,23,106,121,172,53,114,153,203,98,157,25,112,236,102,138,19,105,167,77,
			8,160,167,59,188,39,119,140,165,142,140,237,2,102,195,84,179,157,51,125,10,207,61,163,232,24,108,0,89,102,32,140,
		],
		[
			12,161,235,97,174,197,204,27,238,159,220,51,169,185,51,248,229,14,226,254,173,154,222,129,245,175,38,176,28,62,174,21,
			14,129,211,238,171,161,207,133,36,82,38,231,95,239,220,191,127,186,70,128,58,107,14,67,103,14,108,209,56,49,55,187,
		],
		[
			19,236,239,137,235,113,12,237,14,137,247,77,248,208,213,100,31,143,178,206,245,179,219,145,141,135,221,149,103,65,92,71,
			25,43,76,208,117,232,230,58,166,121,74,135,252,13,77,171,138,23,187,104,157,182,72,174,187,133,96,206,178,119,105,228,
		],
		[
			0,240,175,208,127,214,83,43,138,61,24,91,184,238,170,250,57,165,94,168,99,20,231,219,131,229,179,163,206,198,7,115,
			10,58,232,77,144,123,229,19,166,124,195,132,110,3,160,14,136,36,153,144,95,113,189,4,184,72,133,76,226,207,222,64,
		],
		[
			38,102,37,243,170,94,210,210,113,56,216,234,75,183,201,158,14,236,65,34,242,42,33,127,57,66,27,156,59,204,57,162,
			5,92,32,140,102,80,48,6,28,202,114,155,114,58,144,97,189,84,193,197,60,13,43,17,233,165,187,8,214,53,177,132,
		],
		[
			33,202,211,197,112,227,208,101,55,153,251,165,247,47,205,188,55,168,113,238,104,0,233,161,189,60,85,111,129,235,16,205,
			22,222,114,71,188,68,235,90,102,195,136,190,40,64,182,231,243,41,185,255,118,97,119,224,215,223,149,23,210,78,150,98,
		],
		[
			35,197,109,159,3,24,144,86,199,249,15,224,70,151,173,209,178,1,220,149,148,115,245,166,175,111,61,170,192,98,117,223,
			26,98,205,197,53,68,133,27,28,37,170,208,26,203,115,236,181,86,93,106,164,218,120,71,179,152,172,126,0,111,125,109,
		],
		[
			29,246,242,86,44,77,1,170,224,90,186,145,237,223,99,155,107,90,221,155,98,202,195,162,30,43,75,53,214,163,212,55,
			6,59,72,6,128,165,177,99,177,1,171,207,211,179,144,208,140,207,207,5,167,82,160,158,183,184,20,217,245,184,120,95,
		],
		[
			44,71,133,140,78,88,69,49,125,49,145,127,244,251,193,239,75,72,247,102,244,180,184,46,207,181,225,134,177,221,92,228,
			34,204,0,54,38,245,1,124,49,41,18,85,252,154,217,248,118,58,52,98,17,148,188,165,48,247,44,252,108,234,244,133,
		],
		[
			27,152,194,6,250,227,88,15,119,198,164,119,250,174,50,53,85,11,96,0,56,181,203,44,38,236,244,2,13,175,177,100,
			8,86,107,15,212,202,112,253,91,167,214,90,79,22,117,47,190,42,15,100,69,57,186,155,42,186,177,182,162,67,230,102,
		],
		[
			26,240,236,236,240,50,198,240,81,30,130,142,238,102,99,6,249,79,27,149,184,27,63,111,135,214,189,61,182,145,214,217,
			43,60,52,111,85,185,198,11,189,134,115,19,164,3,20,198,59,34,109,154,33,195,192,150,199,170,24,145,195,183,175,203,
		],
		[
			33,156,23,215,145,229,71,210,25,201,105,85,143,13,22,54,35,125,101,92,147,80,130,52,8,197,210,17,187,204,139,132,
			25,72,214,5,237,86,13,65,199,59,207,182,72,158,156,60,33,163,245,241,100,185,8,246,207,112,198,227,230,231,225,235,
		],
		[
			47,3,37,21,247,246,222,84,131,157,49,202,25,217,137,49,108,220,182,244,134,235,192,78,113,123,176,0,198,68,1,65,
			21,73,66,102,55,31,180,233,127,116,169,94,219,25,136,105,235,241,64,200,103,94,193,170,40,47,210,93,176,189,165,199,
		],
		[
			8,27,142,128,13,220,63,40,207,191,158,22,140,164,252,62,13,236,172,215,242,30,84,213,58,134,11,17,90,87,38,117,
			22,237,95,202,48,214,133,88,187,65,165,154,82,205,244,243,25,122,248,68,5,215,67,193,32,152,178,1,136,194,67,104,
		],
	]
};