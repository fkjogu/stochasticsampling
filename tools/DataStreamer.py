from io import SEEK_CUR
import lzma
import msgpack
import cbor
import numpy as np
import os


class Streamer(object):
    """Slicable object, representing all blobs in data file,
    streaming directly from the disk.
    """

    def __init__(self, source_fn, index_fn=None, index=None):
        self.source_fn = source_fn
        ext = os.path.splitext(source_fn)
        if ext is 'msgpack-lzma':
            self.type = 'Msgpack'
        elif ext is 'cbor-lzma':
            self.type = 'CBOR'
        else:
            self.type = None

        if index is None:
            # if index_fn is None:
            #     self.index = self.build_index()
            # else:
            #     self.set_index_from_file(index_fn)

            self.set_index_from_file(index_fn)
        else:
            self.set_index(index)
        self.__file = open(source_fn, 'rb')
        self.metadata = None

    def __del__(self):
        self.__file.close()

    def parse(buffer):
        if self.type is 'Msgpack':
            return msgpack.unpackb(buf, encoding='utf-8')
        elif self.type is 'CBOR':
            return cbor.loads(buf)

    def __getitem__(self, given):
        if isinstance(given, slice):
            data = []

            for (i, s) in zip(self.index[given.start:given.stop:given.step],
                              self.blob_size[given.start:given.stop:given.step]):
                self.__file.seek(int(i))  # convert bit to byte position
                # read blob
                buf = self.__file.read(s)
                buf = lzma.decompress(buf)
                data.append(msgpack.unpackb(buf, encoding='utf-8'))

            return data
        else:
            self.__file.seek(int(self.index[given]))
            buf = self.__file.read(self.blob_size[given])
            buf = lzma.decompress(buf)
            return msgpack.unpackb(buf, encoding='utf-8')

    def generator(self, start=0, step=1, stop=None):
        """Generator that yields simulation output"""

        for (i, s) in zip(self.index[start:stop:step],
                          self.blob_size[start:stop:step]):
            self.__file.seek(int(i))  # convert bit to byte position
            # read blob
            buf = self.__file.read(s)
            buf = lzma.decompress(buf)
            yield msgpack.unpackb(buf, encoding='utf-8')

    def get_index(self):
        return self.index

    def set_index(self, index):
        self.index = index
        self.blob_size = np.diff(
            np.append(self.index, os.path.getsize(self.source_fn))
        ).astype(np.uint64)

    def set_index_from_file(self, file):
        self.set_index(np.fromfile(file, dtype=np.uint64))

    def get_metadata(self):
        if self.metadata is None:
            with open(self.source_fn, 'rb') as f:
                buf = f.read(self.index[0])
                self.metadata = msgpack.unpackb(buf, encoding='utf-8')

        return self.metadata

    def parameter_string(self):
        sim_settings = self.get_metadata()
        return (
            r"$\kappa = {kappa}, \sigma_a = {sa}, \sigma_b = {sb}, b = {b}, d_t = {dt}, d_r = {dr}$",
            {
                'kappa': sim_settings['parameters']['magnetic_reorientation'] /
                sim_settings['parameters']['diffusion']['rotational'],
                'sa': sim_settings['parameters']['stress']['active'],
                'sb': sim_settings['parameters']['stress']['magnetic'],
                'b': sim_settings['parameters']['magnetic_reorientation'],
                'dt': sim_settings['parameters']['diffusion']['translational'],
                'dr': sim_settings['parameters']['diffusion']['rotational']
            }
        )

    def get_scaling(self, gw, start=0, step=1, stop=None):
        vmax = 0

        g = self.generator(start, step, stop)

        for data in g:
            dist = dist_to_concentration3d(data_to_dist(data), gw)
            m = np.max(dist)

            if m > vmax:
                vmax = m

        return m

    def get_length(self):
        return len(self.index)

    def get_coordinates(self):
        bs, gs, gw = get_bs_gs_gw(self.get_metadata())

        return {
            'x': np.linspace(gw['x'] / 2, bs['x'] - gw['x'] / 2, gs['x']),
            'y': np.linspace(gw['y'] / 2, bs['y'] - gw['y'] / 2, gs['y']),
            'z': np.linspace(gw['z'] / 2, bs['z'] - gw['z'] / 2, gs['z']),
            'phi': np.linspace(
                gw['phi'] / 2, bs['phi'] - gw['phi'] / 2, gs['phi']),
            'theta': np.linspace(
                gw['theta'] / 2, bs['theta'] - gw['theta'] / 2, gs['theta'])
        }


def dist_to_concentration2d(dist, gw):
    """Takes an distribution array and returns a concentration
    field by naive integraton of orientation.
    """
    return np.sum(dist, axis=2) * gw['phi']


def data_to_flowfield(data):
    """ Return flow field with [component, x, y, z]. """
    ff = data['flowfield']
    ff = np.array(ff['data']).reshape(ff['dim'])
    return ff


def data_to_magneticfield(data):
    """ Return magnetic field with [component, x, y, z]. """
    mf = data['magneticfield']
    mf = np.array(mf['data']).reshape(mf['dim'])
    return mf


def data_to_dist(data):
    """Takes data dictonary and returns numpy array of sampled
    distribution in the correct shape, with (x, y, angle).
    """
    return np.array(data['distribution']['dist']['data']).reshape(
        data['distribution']['dist']['dim'])


def dist_to_concentration3d(dist, gw):
    """Takes an distribution array and returns a concentration
    field by naive integraton of orientation.
    """
    return np.sum(dist, axis=(3, 4)) * gw['phi'] * gw['theta']


def get_mean_polarisation(dist, gw, gs):
    """Takes distribution and returns expectation value for the polarisation field"""

    phi = np.linspace(0, 2 * np.pi, gs['phi'], endpoint=False) + gw['phi'] / 2
    theta = np.linspace(0, np.pi, gs['theta'],
                        endpoint=False) + gw['theta'] / 2

    ph, th = np.meshgrid(phi, theta, indexing='ij')

    x = np.sin(th) * np.cos(ph)
    y = np.sin(th) * np.sin(ph)
    z = np.cos(th)

    n = dist.shape[0] * dist.shape[1] * dist.shape[2]

    vx = np.sum(dist * x, axis=(3, 4)) * gw['theta'] * gw['phi'] / n
    vy = np.sum(dist * y, axis=(3, 4)) * gw['theta'] * gw['phi'] / n
    vz = np.sum(dist * z, axis=(3, 4)) * gw['theta'] * gw['phi'] / n

    return np.transpose(np.array([vx, vy, vz]), (1, 2, 3, 0))


def get_mean_orientation(dist, gw, gs):
    """Takes distribution and returns mean orientation per particle vector field. Caution: This can return NaN"""

    phi = np.linspace(0, 2 * np.pi, gs['phi'], endpoint=False) + gw['phi'] / 2
    theta = np.linspace(0, np.pi, gs['theta'],
                        endpoint=False) + gw['theta'] / 2

    ph, th = np.meshgrid(phi, theta, indexing='ij')

    x = np.sin(th) * np.cos(ph)
    y = np.sin(th) * np.sin(ph)
    z = np.cos(th)

    c = dist_to_concentration3d(dist, gw)

    vx = np.sum(dist * x, axis=(3, 4)) * gw['theta'] * gw['phi'] / c
    vy = np.sum(dist * y, axis=(3, 4)) * gw['theta'] * gw['phi'] / c
    vz = np.sum(dist * z, axis=(3, 4)) * gw['theta'] * gw['phi'] / c

    return np.transpose(np.array([vx, vy, vz]), (1, 2, 3, 0))


def get_bs_gs_gw(sim_settings):
    bs = sim_settings['simulation']['box_size']
    gs = sim_settings['simulation']['grid_size']

    bs['phi'] = 2 * np.pi
    bs['theta'] = np.pi

    gw = {
        'x': bs['x'] / gs['x'],
        'y': bs['y'] / gs['y'],
        'z': bs['z'] / gs['z'],
        'phi': 2 * np.pi / gs['phi'],
        'theta': np.pi / gs['theta']
    }

    return bs, gs, gw
