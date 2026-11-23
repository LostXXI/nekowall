// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright © 2026 Lost (LostXXI) — NekoWall — https://github.com/LostXXI/nekowall
// Vista previa de vídeo para «Pegar enlace». Va en su propio archivo y se carga con un Loader: si
// falta Qt Multimedia (qt6-multimedia), el Loader falla sin romper la ventana y se ve la miniatura.
import QtQuick
import QtMultimedia

Item {
    id: root
    property url source
    signal failed(string message)
    readonly property bool ready: player.playbackState === MediaPlayer.PlayingState
                                  && player.mediaStatus !== MediaPlayer.LoadingMedia

    MediaPlayer {
        id: player
        source: root.source
        videoOutput: out
        loops: MediaPlayer.Infinite      // sin audioOutput: en silencio
        onErrorOccurred: (error, message) => root.failed(message)
        // La ventana pone el enlace después de crear el reproductor: arranca cuando el propio
        // reproductor ya lo tiene (no antes).
        onSourceChanged: if (source.toString() !== "") play()
    }
    VideoOutput {
        id: out
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectFit
    }
    Component.onCompleted: if (source.toString() !== "") player.play()
}
