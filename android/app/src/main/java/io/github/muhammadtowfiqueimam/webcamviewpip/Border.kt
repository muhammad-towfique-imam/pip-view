package io.github.muhammadtowfiqueimam.webcamviewpip

/** The PiP border options, in menu order. */
enum class Border(val label: String, val argb: Int?) {
    NONE("None", null),
    WHITE("White", 0xFFFFFFFF.toInt()),
    GREEN("Green", 0xFF00FF00.toInt()),
    RED("Red", 0xFFFF0000.toInt()),
    BLUE("Blue", 0xFF0000FF.toInt()),
    YELLOW("Yellow", 0xFFFFFF00.toInt());

    companion object {
        /** Border line width in dp. Phone screens have a high density, so 3 physical pixels are too thin. */
        const val WIDTH_DP = 3f

        /** The border with this label. An unknown or missing label gives [NONE]. */
        fun parse(label: String?): Border = entries.firstOrNull { it.label == label } ?: NONE
    }
}
