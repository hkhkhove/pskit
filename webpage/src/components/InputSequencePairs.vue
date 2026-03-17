<script setup>
import { computed, ref, watch } from "vue";

const MAX_PAIRS = 100;

const pairs = defineModel("pairs", {
    type: Array,
    default: () => [{ protein: "", nucleic: "" }],
});

const pasteText = defineModel("pasteText", {
    type: String,
    default: "",
});

const parseWarning = defineModel("parseWarning", {
    type: String,
    default: "",
});

const syncingFromText = ref(false);
const syncingFromPairs = ref(false);
const isEditingPasteText = ref(false);

const hasPairs = computed(() => Array.isArray(pairs.value) && pairs.value.length > 0);


function normalizePair(pair = {}) {
    return {
        protein: String(pair.protein || ""),
        nucleic: String(pair.nucleic || ""),
    };
}

function ensureAtLeastOneRow() {
    if (!hasPairs.value) {
        pairs.value = [normalizePair()];
    }
}

ensureAtLeastOneRow();

function removePair(index) {
    const next = pairs.value.filter((_, i) => i !== index);
    pairs.value = next.length > 0 ? next : [normalizePair()];
}

function setExample() {
    pasteText.value = [
        "MEYASDASLDPEAPWPPAPRARACRVLPWALVAGLLLLLLLAAACAVFLACPWAVSGARASPGSAASPRLREGPELSPDDPAGLLDLRQGMFAQLVAQNVLLIDGPLSWYSDPGLAGVSLTGGLSYKEDTKELVVAKAGVYYVFFQLELRRVVAGEGSGSVSLALHLQPLRSAAGAAALALTVDLPPASSEARNSAFGFQGRLLHLSAGQRLGVHLHTEARARHAWQLTQGATVLGLFRVTPEIPAGLPSPRSE,UAAUACGACUCACUAUAGGGAGGACGAUGCGGACAUAGUAAUGACACGGAGGAUGGAGAAAAAACAGCCAUCUCUUGACGGUUCGGGCGAGUCGUCUG",
        "QDRPIKFSTEGATSQSYKQFIEALRERLRGGLIHDIPVLPDPTTLQERNRYITVELSNSDTESIEVGIDVTNAYVVAYRAGTQSYFLRDAPSSASDYLFTGTDQHSLPFYGTYGDLERWAHQSRQQIPLGLQALTHGISFFRSGGNDNEEKARTLIVIIQMVAEAARFRYISNRVRVSIQTGTAFQPDAAMISLENNWDNLSRGVQESVQDTFPNQVTLTNIRNEPVIVDSLSHPTVAVLALMLFVCNPPNANQSPLLIRSIVEKSKICSSRYEPTVRIGGRDGMCVDVYDNGYHNGNRIIMWKCKDRLEENQLWTLKSDKTIRSNGKCLTTYGYAPGSYVMIYDCTSAVAEATYWEIWDNGTIINPKSALVLSAESSSMGGTLTVQTNEYLMRQGWRTGNNTSPFVTSISGYSDLCMQAQGSNVWMADCDSNKKEQQWALYTDGSIRSVQNTNNCLTSKDHKQGSTILLMGCSNGWASQRWVFKNDGSIYSLYDDMVMDVKGSDPSLKQIILWPYTGKPNQIWLTLF,CAGCTCAGAAGCTTGATCCTGTGAGCGAAAATCCGGAGTAGAGGAGCAGCTGGGTGCTGACTCGAAGTCGTGCATCTGCA",
        "MAAGTAVGAWVLVLSLWGAVVGAQNITARIGEPLVLKCKGAPKKPPQRLEWKLNTGRTEAWKVLSPQGGGPWDSVARVLPNGSLFLPAVGIQDEGIFRCQAMNRNGKETKSNYRVRVYQIPGKPEIVDSASELTAGVPNKVGTCVSEGSYPAGTLSWHLDGKPLVPNEKGVSVKEQTRRHPETGLFTLQSELMVTPARGGDPRPTFSCSFSPGLPRHRALRTAPIQPRVWEPVPLEEVQLVVEPEGGAVAPGGTVTLTCEVPAQPSPQIHWMKDGVPLPLPPSPVLILPEIGPQDQGTYSCVATHSSHGPQESRAVSISIIEPGEEGPTAGSVGGSGLGTLALALGILGGLGTAALLIGVILWQRRQRRGEERKAPENQEEEEERAELNQSEEPEAGESSTGGP,TCGGATGCGCCGAGTCTCCGTTTACCTTCGT",
        "MSSSQKKAGGKAGKPTKRSQNYAALRKAQLPKPPALKVPVVKPTNTILPQTGCVWQSLGTPLSLSSFNGLGARFLYSFLKDFVGPRILEEDLIYRMVFSITPSHAGTFCLTDDVTTEDGRAVAHGNPMQEFPHGAFHANEKFGFELVFTAPTHAGMQNQNFKHSYAVALCLDFDAQPEGSKNPSFRFNEVWVERKAFPRAGPLRSLITVGLFDEADDLDRH,GCGCAGGCGGGGTTTGACTTCGAGGCCG",
        "MLDLFADAEPWQEPLAAGAVILRRFAFNAAEQLIRDINDVASQSPFRQMVTPGGYTMSVAMTNCGHLGWTTHRQGYLYSPIDPQTNKPWPAMPQSFHNLCQRAATAAGYPDFQPDACLINRYAPGAKLSLHQDKDEPDLRAPIVSVSLGLPAIFQFGGLKRNDPLKRLLLEHGDVVVWGGESRLFYHGIQPLKAGFHPLTIDCRYNLTFRQAGKKE,CTCCTCTGACTGTAACCACGGACTGCTGATGAGTCACTTTAACGTGGAGCAAAGATTAAAGCATAGGTAGTCCAGAAGCC",
        "MTVFLSFAFLAAILTHIGCSNQRRSPENSGRRYNRIQHGQCAYTFILPEHDGNCRESTTDQYNTNALQRDAPHVEPDFSSQKLQHLEHVMENYTQWLQKLENYIVENMKSEMAQIQQNAVQNHTATMLEIGTSLLSQTAEQTRKLTDVETQVLNQTSRLEIQLLENSLSTYKLEKQLLQQTNEILKIHEKNSLLEHKILEMEGKHKEELDTLKEEKENLQGLVTRQTYIIQELEKQLNRATTNNSVLQKQQLELMDTVHNLVNLCTKEGVLLKGGKREEEKPFRDCADVYQAGFNKSGIYTIYINNMPEPKKVFCNMDVNGGGWTVIQHREDGSLDFQRGWKEYKMGFGNPSGEYWLGNEFIFAITSQRQYMLRIELMDWEGNRAYSQYDRFHIGNEKQNYRLYLKGHTGTAGKQSSLILHGADFSTKDADNDNCMCKCALMLTGGWWFDACGPSNLNGMFYTAGQNHGKLNGIKWHYFKGPSYSLRSTTMMIRPLDF,GAUGUUUCGAAUGUUGCGGGUGAGACACAGCAUGACAAACUACCGUGUCA",
        "MWQIVFFTLSCDLVLAAAYNNFRKSMDSIGKKQYQVQHGSCSYTFLLPEMDNCRSSSSPYVSNAVQRDAPLEYDDSVQRLQVLENIMENNTQWLMKLENYIQDNMKKEMVEIQQNAVQNQTAVMIEIGTNLLNQTAEQTRKLTDVEAQVLNQTTRLELQLLEHSLSTNKLEKQILDQTSEINKLQDKNSFLEKKVLAMEDKHIIQLQSIKEEKDQLQVLVSKQNSIIEELEKKIVTATVNNSVLQKQQHDLMETVNNLLTMMSTSNSAKDPTVAKEEQISFRDCAEVFKSGHTTNGIYTLTFPNSTEEIKAYCDMEAGGGGWTIIQRREDGSVDFQRTWKEYKVGFGNPSGEYWLGNEFVSQLTNQQRYVLKIHLKDWEGNEAYSLYEHFYLSSEELNYRIHLKGLTGTAGKISSISQPGNDFSTKDGDNDKCICKCSQMLTGGWWFDACGPSNLNGMYYPQRQNTNKFNGIKWYYWKGSGYSLKATTMMIRPADF,CUCUUUUUGUCCCCGCACGUUGAACUCCUGUCCCUCUACU",
        "MQRTKEAVKASDGNLLGDPGRIPLSKRESIKWQRPRFTRQALMRCCLIKWILSSAAPQGSDSSDSELELSTVRHQPEGLDQLQAQTKFTKKELQSLYRGFKNECPTGLVDEDTFKLIYSQFFPQGDATTYAHFLFNAFDADGNGAIHFEDFVVGLSILLRGTVHEKLKWAFNLYDINKDGCITKEEMLAIMKSIYDMMGRHTYPILREDAPLEHVERFFQKMDRNQDGVVTIDEFLETCQKDENIMNSMQLFENVI,GAGGACGAUGCGGACUAGCCUCAUCAGCUCAUGUGCCCCUC",
        "MNRGVPFRHLLLVLQLALLPAATQGKKVVLGKKGDTVELTCTASQKKSIQFHWKNSNQIKILGNQGSFLTKGPSKLNDRADSRRSLWDQGNFPLIIKNLKIEDSDTYICEVEDQKEEVQLLVFGLTANSDTHLLQGQSLTLTLESPPGSSPSVQCRSPRGKNIQGGKTLSVSQLELQDSGTWTCTVLQNQKKVEFKIDIVVLAFQKASSIVYKKEGEQVEFSFPLAFTVEKLTGSGELWWQAERASSSKSWITFDLKNKEVSVKRVTQDPKLQMGKKLPLHLTLPQALPQYAGSGNLTLALEAKTGKLHQEVNLVVMRATQLQKNLTCEVWGPTSPKLMLSLKLENKEAKVSKREKAVWVLNPEAGMWQCLLSDSGQVLLESNIKVLPTWSTPVQPMALIVLGGVAGLLLFIGLGIFFCVRCRHRRRQAERMSQIKRLLSEKKTCQCPHRFQKTCSPI,GGCTGTTGTGAGCCTCCTCCCAGAGGGAAGACTTTAGGTTCGGTTCACGTCCCGCTTATTCTTACTCCC",
        "MDKFWWHAAWGLCLVPLSLAQIDLNITCRFAGVFHVEKNGRYSISRTEAADLCKAFNSTLPTMAQMEKALSIGFETCRYGFIEGHVVIPRIHPNSICAANNTGVYILTSNTSQYDTYCFNASAPPEEDCTSVTDLPNAFDGPITITIVNRDGTRYVQKGEYRTNPEDIYPSNPTDDDVSSGSSSERSSTSGGYIFYTFSTVHPIPDEDSPWITDSTDRIPATTLMSTSATATETATKRQETWDWFSWLFLPSESKNHLHTTTQMAGTSSNTISAGWEPNEENEDERDRHLSFSGSGIDDDEDFISSTISTTPRAFDHTKQNQDWTQWNPSHSNPEVLLQTTTRMTDVDRNGTTAYEGNWNPEAHPPLIHHEHHEEEETPHSTSTIQATPSSTTEETATQKEQWFGNRWHEGYRQTPKEDSHSTTGTAAASAHTSHPMQGRTTPSPEDSSWTDFFNPISHPMGRGHQAGRRMDMDSSHSITLQPTANPNTGLVEDLDRTGPLSMTTQQSNSQSFSTSHEGLEEDKDHPTTSTLTSSNRNDVTGGRRDPNHSEGSTTLLEGYTSHYPHTKESRTFIPVTSAKTGSFGVTAVTVGDSNSNVNRSLSGDQDTFHPSGGSHTTHGSESDGHSHGSQEGGANTTSGPIRTPQIPEWLIILASLLALALILAVCIAVNSRRRCGQKKKLVINSGNGAVEDRKPSGLNGEASKSQEMVHLVNKESSETPDQFMTADETRNLQNVDMKIGV,TGCAGATGCAAGGTAACCATATCCAAAGCA",
    ].join("\n");
    parseWarning.value = "";
}

function parseLine(line) {
    const trimmed = line.trim();
    if (!trimmed) return null;

    if (trimmed.includes(",")) {
        const parts = trimmed.split(",");
        if (parts.length >= 2) {
            return normalizePair({ protein: parts[0].trim(), nucleic: parts[1].trim() });
        }
    }

    if (trimmed.includes("\t")) {
        const parts = trimmed.split("\t");
        if (parts.length >= 2) {
            return normalizePair({ protein: parts[0].trim(), nucleic: parts[1].trim() });
        }
    }

    const parts = trimmed.split(/\s+/);
    if (parts.length >= 2) {
        return normalizePair({ protein: parts[0].trim(), nucleic: parts[1].trim() });
    }

    return null;
}

function parseBulkTextToPairs() {
    const lines = String(pasteText.value || "")
        .split(/\r?\n/)
        .filter((line) => line.trim() !== "");

    if (lines.length === 0) {
        syncingFromText.value = true;
        pairs.value = [normalizePair()];
        syncingFromText.value = false;
        parseWarning.value = "";
        return;
    }

    const parsed = [];
    let failed = 0;
    for (const line of lines) {
        const pair = parseLine(line);
        if (pair && pair.protein && pair.nucleic) parsed.push(pair);
        else failed += 1;
    }

    if (parsed.length === 0) {
        parseWarning.value = "Unable to parse any line. Use one pair per line: protein,nucleic.";
        return;
    }

    let overflow = 0;
    if (parsed.length > MAX_PAIRS) {
        overflow = parsed.length - MAX_PAIRS;
    }

    syncingFromText.value = true;
    pairs.value = parsed.slice(0, MAX_PAIRS);
    syncingFromText.value = false;

    const warnings = [];
    if (failed > 0) warnings.push(`${failed} line(s) were skipped because they could not be parsed.`);
    if (overflow > 0) warnings.push(`${overflow} line(s) were ignored because only ${MAX_PAIRS} pairs are allowed.`);
    parseWarning.value = warnings.join(" ");
}

function pairsToText(list) {
    return (list || [])
        .map((item) => {
            const p = String(item?.protein || "").trim();
            const n = String(item?.nucleic || "").trim();
            if (!p && !n) return "";
            return `${p},${n}`;
        })
        .filter((line) => line !== "")
        .join("\n");
}

watch(
    () => pasteText.value,
    () => {
        if (syncingFromPairs.value) return;
        parseBulkTextToPairs();
    },
    { immediate: true },
);

watch(
    () => pairs.value,
    (val) => {
        if (syncingFromText.value || isEditingPasteText.value) return;
        syncingFromPairs.value = true;
        pasteText.value = pairsToText(val);
        syncingFromPairs.value = false;
    },
    { deep: true },
);
</script>

<template>
    <div class="w-full">
        <div class="my-4 flex items-center justify-between">
            <span class="text-xl font-semibold text-gray-900 dark:text-gray-400">Input Sequence Pairs</span>
            <button type="button"
                class="text-xs rounded-md border border-gray-300 px-2.5 py-1.5 text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
                @click="setExample">Use Example</button>
        </div>

        <div class="rounded-lg border border-gray-200 bg-gray-50 p-3 dark:border-gray-700 dark:bg-gray-800/50">
            <p class="text-xs text-gray-600 dark:text-gray-300">Paste one pair per line, separated by comma, tab, or
                spaces.
            </p>
            <textarea v-model="pasteText" rows="4" placeholder="MSEQVENCEKQ...,AUGCUAUGCUA"
                @focus="isEditingPasteText = true" @blur="isEditingPasteText = false"
                class="h-64 mt-2 w-full rounded-lg border border-gray-300 bg-white p-2.5 text-sm text-gray-900 focus:border-blue-400 focus:outline-none focus:ring-1 focus:ring-blue-400 dark:border-gray-600 dark:bg-gray-700 dark:text-white"></textarea>
            <div class="mt-2 flex items-center gap-2">
                <span v-if="parseWarning" class="text-xs text-amber-700 dark:text-amber-400">{{ parseWarning }}</span>
            </div>
        </div>

        <div class="mt-4 max-h-64 overflow-auto rounded-lg border border-gray-200 dark:border-gray-700">
            <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                <thead class="bg-gray-100 dark:bg-gray-700 sticky top-0 z-10">
                    <tr>
                        <th class="px-3 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">#</th>
                        <th class="px-3 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Protein
                            Sequence</th>
                        <th class="px-3 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Nucleic
                            Sequence</th>
                        <th class="px-3 py-2 text-left text-xs font-semibold text-gray-700 dark:text-gray-300">Action
                        </th>
                    </tr>
                </thead>
                <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
                    <tr v-for="(pair, index) in pairs" :key="index" class="bg-white dark:bg-gray-800">
                        <td class="px-3 py-2 text-xs text-gray-700 dark:text-gray-300">{{ index + 1 }}</td>
                        <td class="px-3 py-2">
                            <textarea v-model="pair.protein" rows="2" placeholder="Protein sequence" maxlength="1000"
                                class="w-full rounded-md border border-gray-300 bg-white p-2 text-xs font-mono text-gray-900 focus:border-blue-400 focus:outline-none focus:ring-1 focus:ring-blue-400 dark:border-gray-600 dark:bg-gray-700 dark:text-white"></textarea>
                        </td>
                        <td class="px-3 py-2">
                            <textarea v-model="pair.nucleic" rows="2" placeholder="DNA/RNA sequence" maxlength="1000"
                                class="w-full rounded-md border border-gray-300 bg-white p-2 text-xs font-mono text-gray-900 focus:border-blue-400 focus:outline-none focus:ring-1 focus:ring-blue-400 dark:border-gray-600 dark:bg-gray-700 dark:text-white"></textarea>
                        </td>
                        <td class="px-3 py-2">
                            <button type="button"
                                class="rounded-md border border-red-200 bg-red-50 px-2.5 py-1 text-xs text-red-700 hover:bg-red-100 dark:border-red-800 dark:bg-red-900/20  dark:text-red-300"
                                @click="removePair(index)">Remove</button>
                        </td>
                    </tr>
                </tbody>
            </table>
        </div>

        <div class="mt-2 text-xs text-gray-500 dark:text-gray-300">{{ pairs.length }}/{{ MAX_PAIRS }} pairs</div>

    </div>
</template>
