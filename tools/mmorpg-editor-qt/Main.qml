import QtQuick 6.8
import QtQuick.Controls 6.8
import QtQuick.Layouts 6.8
import QtQuick3D 6.8

Item {
    id: root

    Rectangle {
        anchors.fill: parent
        color: "#171b22"
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "Terrain preview"
                color: "#e8edf5"
                font.pixelSize: 22
            }
            Item { Layout.fillWidth: true }
            Label {
                text: "Qt 6 Quick / Quick3D"
                color: "#9aa8bb"
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 12

            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: "#202733"
                radius: 6

                View3D {
                    id: terrainViewport
                    anchors.fill: parent
                    anchors.margins: 12
                    function syncViewport() {
                        var origin = terrainViewport.mapToItem(null, 0, 0)
                        tabletBridge.setViewport(origin.x, origin.y,
                                                 terrainViewport.width,
                                                 terrainViewport.height)
                    }
                    Component.onCompleted: syncViewport()
                    onWidthChanged: syncViewport()
                    onHeightChanged: syncViewport()
                    environment: SceneEnvironment {
                        clearColor: "#202733"
                        backgroundMode: SceneEnvironment.Color
                    }
                    PerspectiveCamera {
                        id: camera
                        position: Qt.vector3d(0, 180, 260)
                        eulerRotation.x: -32
                    }
                    DirectionalLight {
                        eulerRotation.x: -45
                        eulerRotation.y: -30
                        brightness: 1.4
                    }
                    Model {
                        geometry: terrainGeometry
                        materials: PrincipledMaterial {
                            baseColor: "#4e7a52"
                            roughness: 0.9
                        }
                    }
                }

                // The native QQuickView receives tablet events over this
                // viewport. Mouse input is a pressure=1.0 fallback.
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.NoButton
                    hoverEnabled: true
                }
            }

            Frame {
                Layout.preferredWidth: 310
                Layout.fillHeight: true
                padding: 14

                ColumnLayout {
                    anchors.fill: parent
                    spacing: 10
                    Label { text: "Input diagnostics"; font.bold: true }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: tabletBridge.lastInput
                        color: "#2d6a4f"
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: "Rust core: " + tabletBridge.coreStatus
                        color: "#53718f"
                    }
                    Label {
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                        text: "Draw with a pen or mouse over the preview. Tablet-generated compatibility mouse events are suppressed."
                        color: "#596579"
                    }
                    Item { Layout.fillHeight: true }
                    Button {
                        text: "Raise"; Layout.fillWidth: true
                        onClicked: tabletBridge.selectBrush("raise")
                    }
                    Button {
                        text: "Lower"; Layout.fillWidth: true
                        onClicked: tabletBridge.selectBrush("lower")
                    }
                    Button {
                        text: "Smooth"; Layout.fillWidth: true
                        onClicked: tabletBridge.selectBrush("smooth")
                    }
                    Label { text: "Radius" }
                    Slider {
                        id: radiusSlider
                        Layout.fillWidth: true
                        from: 1; to: 12; value: 5; stepSize: 0.5
                        onValueChanged: tabletBridge.setBrushSettings(value, strengthSlider.value)
                    }
                    Label { text: "Strength" }
                    Slider {
                        id: strengthSlider
                        Layout.fillWidth: true
                        from: 0.1; to: 1; value: 0.8; stepSize: 0.05
                        onValueChanged: tabletBridge.setBrushSettings(radiusSlider.value, value)
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Button {
                            text: "Undo"; Layout.fillWidth: true
                            onClicked: tabletBridge.undoTerrain()
                        }
                        Button {
                            text: "Redo"; Layout.fillWidth: true
                            onClicked: tabletBridge.redoTerrain()
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        Button {
                            text: "Save"; Layout.fillWidth: true
                            onClicked: tabletBridge.saveTerrain()
                        }
                        Button {
                            text: "Reload"; Layout.fillWidth: true
                            onClicked: tabletBridge.reloadTerrain()
                        }
                        Button {
                            text: "Capture"; Layout.fillWidth: true
                            onClicked: tabletBridge.captureStroke()
                        }
                    }
                }
            }
        }

        Label {
            Layout.fillWidth: true
            text: "Native lifecycle: proximity / press / move / release / focus-loss cancellation; events route into the Rust TerrainEditor bridge."
            color: "#9aa8bb"
        }
    }
}
